//! v2.2.1 minimal-proof-set compiler.
//!
//! Replaces the hybrid top-k packer in `context.rs` for callers that opt in
//! via the new `context_compile_v2` MCP tool. The compiler solves the PCKC
//! compilation objective:
//!
//!   Find the smallest set of current-valid claims whose proof is sufficient
//!   to answer the objective, subject to a hard token budget.
//!
//! Unlike the packer, the compiler:
//!   - filters out `model_derived` authority claims up front (the belief
//!     gate already rejects them, but defense-in-depth),
//!   - drops superseded claims and claims whose `valid_to` has passed,
//!   - runs weighted set-cover over sub-goals inferred from the objective,
//!   - refuses to silently truncate — if the minimal cover exceeds budget,
//!     it surfaces the uncovered sub-goals in `ContextPack.unknowns` as
//!     `"proof_gap: <sub_goal>"` markers instead.
//!
//! See `docs/research/v2.2.1-pckc/DESIGN.md` §4.

use std::collections::{BTreeMap, BTreeSet};

use chum_mem_contracts::{
    AuthorityClass, ContextBuildResponse, ContextItem, ContextPack, ContextSourceClass, MemoryType,
    ProofHandle, RetrievalIntent, TokenUsage, VerificationStatus,
};

/// Atomic sub-goals the compiler tries to cover. Inferred from the objective
/// string via lightweight keyword matching (the ranker's intent inference
/// already handles coarser routing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
enum SubGoal {
    Decisions,
    ActiveTasks,
    Facts,
    Bugs,
    Fixes,
    Constraints,
    Implementation,
    OpenQuestions,
}

impl SubGoal {
    fn label(self) -> &'static str {
        match self {
            SubGoal::Decisions => "decisions",
            SubGoal::ActiveTasks => "active_tasks",
            SubGoal::Facts => "facts",
            SubGoal::Bugs => "bugs",
            SubGoal::Fixes => "fixes",
            SubGoal::Constraints => "constraints",
            SubGoal::Implementation => "implementation",
            SubGoal::OpenQuestions => "open_questions",
        }
    }
}

/// Parse the objective into sub-goals. Always includes the core trio
/// (decisions, active tasks, facts). Expands based on objective keywords.
fn infer_sub_goals(objective: &str) -> BTreeSet<SubGoal> {
    let mut goals = BTreeSet::new();
    goals.insert(SubGoal::Decisions);
    goals.insert(SubGoal::ActiveTasks);
    goals.insert(SubGoal::Facts);

    let lower = objective.to_lowercase();
    if lower.contains("bug")
        || lower.contains("error")
        || lower.contains("debug")
        || lower.contains("fail")
    {
        goals.insert(SubGoal::Bugs);
        goals.insert(SubGoal::Fixes);
    }
    if lower.contains("constraint")
        || lower.contains("must ")
        || lower.contains("requirement")
        || lower.contains("invariant")
    {
        goals.insert(SubGoal::Constraints);
    }
    if lower.contains("implement")
        || lower.contains("refactor")
        || lower.contains("build ")
        || lower.contains("code")
        || lower.contains("module")
    {
        goals.insert(SubGoal::Implementation);
    }
    if lower.contains("question")
        || lower.contains("unknown")
        || lower.contains("unclear")
        || lower.contains("open ")
    {
        goals.insert(SubGoal::OpenQuestions);
    }

    goals
}

/// Which sub-goals does a claim cover?
fn coverage(item: &ContextItem) -> BTreeSet<SubGoal> {
    let mut covered = BTreeSet::new();
    // Only memory-sourced claims contribute to sub-goal coverage.
    if item.source_class != ContextSourceClass::Memory {
        return covered;
    }
    match item.memory_type {
        MemoryType::Decision => {
            covered.insert(SubGoal::Decisions);
        }
        MemoryType::Task => {
            covered.insert(SubGoal::ActiveTasks);
        }
        MemoryType::Fact => {
            covered.insert(SubGoal::Facts);
        }
        MemoryType::Bug => {
            covered.insert(SubGoal::Bugs);
        }
        MemoryType::Fix => {
            covered.insert(SubGoal::Fixes);
        }
        MemoryType::Constraint => {
            covered.insert(SubGoal::Constraints);
        }
        MemoryType::ImplementationDetail => {
            covered.insert(SubGoal::Implementation);
        }
        MemoryType::OpenQuestion => {
            covered.insert(SubGoal::OpenQuestions);
        }
        MemoryType::Summary | MemoryType::Risk | MemoryType::ChangeLog => {}
    }
    covered
}

/// Freshness factor — claims with a set `valid_to` are already stale.
fn freshness(item: &ContextItem) -> f64 {
    match item.valid_to.as_deref() {
        Some(value) if !value.is_empty() => 0.35,
        _ => 1.0,
    }
}

/// Authority factor — rank verified evidence highest, reject model-derived.
fn authority(item: &ContextItem) -> f64 {
    match item.authority_class {
        Some(AuthorityClass::Repository) => 1.0,
        Some(AuthorityClass::UserConfirmed) => 1.0,
        Some(AuthorityClass::ToolVerified) => 0.95,
        Some(AuthorityClass::TestVerified) => 0.95,
        Some(AuthorityClass::SessionDerived) => 0.55,
        Some(AuthorityClass::ModelDerived) => 0.0,
        None => 0.5,
    }
}

/// Proof density factor — claims with ≥2 proof handles are weighted higher.
fn proof_density(item: &ContextItem) -> f64 {
    let n = item.proof_handles.len() as f64;
    (n / 2.0).min(1.0).max(0.25)
}

fn weight(item: &ContextItem) -> f64 {
    freshness(item) * authority(item) * proof_density(item)
}

/// Filter: drop items that can never contribute to a durable cover.
fn is_admissible(item: &ContextItem) -> bool {
    // Repository and session-graph items are context scaffolding, not
    // sub-goal claims. They still contribute to the ContextPack's
    // `repository_knowledge` / `session_continuity` buckets after the
    // cover is chosen, but they are not eligible for the cover itself.
    if item.source_class != ContextSourceClass::Memory {
        return true;
    }
    if matches!(item.authority_class, Some(AuthorityClass::ModelDerived)) {
        return false;
    }
    if matches!(
        item.verification_status,
        Some(VerificationStatus::Contradicted | VerificationStatus::Unverified)
    ) {
        return false;
    }
    if item.superseded_by.is_some() {
        return false;
    }
    true
}

/// Compile a minimal proof set for the given objective.
///
/// Returns a `ContextBuildResponse` so it can be a drop-in replacement for
/// `build_context_pack` at the API layer.
pub fn compile_minimal_proof_set(
    objective: &str,
    items: &[ContextItem],
    budget: u32,
    retrieval_intent: RetrievalIntent,
) -> ContextBuildResponse {
    let sub_goals = infer_sub_goals(objective);
    let admissible: Vec<ContextItem> = items
        .iter()
        .filter(|item| is_admissible(item))
        .cloned()
        .collect();

    // Deduplicate candidates by claim_key before cover.
    let mut seen_keys: BTreeSet<String> = BTreeSet::new();
    let candidates: Vec<ContextItem> = admissible
        .into_iter()
        .filter(|item| {
            let key = item.claim_key.clone().unwrap_or_else(|| {
                format!(
                    "{:?}:{}:{}",
                    item.source_class,
                    item.title.trim().to_lowercase(),
                    item.summary.trim().to_lowercase()
                )
            });
            seen_keys.insert(key)
        })
        .collect();

    // ── Stage 1: weighted set-cover ────────────────────────────────
    let mut uncovered: BTreeSet<SubGoal> = sub_goals.clone();
    let mut selected: Vec<ContextItem> = Vec::new();
    let mut selected_ids: BTreeSet<String> = BTreeSet::new();
    let mut used: u32 = 0;

    while !uncovered.is_empty() {
        let mut best: Option<(usize, f64)> = None;
        for (idx, item) in candidates.iter().enumerate() {
            let dedupe_key = item.claim_key.clone().unwrap_or_else(|| {
                format!("{:?}:{}", item.source_class, item.title)
            });
            if selected_ids.contains(&dedupe_key) {
                continue;
            }
            let cov = coverage(item);
            let newly_covered = cov.intersection(&uncovered).count();
            if newly_covered == 0 {
                continue;
            }
            if item.tokens == 0 {
                continue;
            }
            let score = (newly_covered as f64 * weight(item)) / item.tokens as f64;
            match best {
                None => best = Some((idx, score)),
                Some((_, prev_score)) if score > prev_score => best = Some((idx, score)),
                _ => {}
            }
        }

        let Some((idx, _score)) = best else {
            break; // no candidate covers any remaining sub-goal
        };

        let pick = &candidates[idx];
        if used.saturating_add(pick.tokens) > budget {
            // Hard ceiling hit. Do NOT truncate. Surface the gap.
            break;
        }
        used = used.saturating_add(pick.tokens);
        for goal in coverage(pick) {
            uncovered.remove(&goal);
        }
        let dedupe_key = pick.claim_key.clone().unwrap_or_else(|| {
            format!("{:?}:{}", pick.source_class, pick.title)
        });
        selected_ids.insert(dedupe_key);
        selected.push(pick.clone());
    }

    // ── Stage 2: fill remaining budget with high-priority context ──
    //
    // After the minimal cover, the budget may still have room. Spend it on
    // repository knowledge, session-graph context, and unsuperseded claims
    // that didn't make the minimal cover but reinforce it.
    let mut filler: Vec<ContextItem> = items
        .iter()
        .filter(|item| {
            let key = item.claim_key.clone().unwrap_or_else(|| {
                format!("{:?}:{}", item.source_class, item.title)
            });
            !selected_ids.contains(&key) && is_admissible(item)
        })
        .cloned()
        .collect();
    filler.sort_by(|a, b| {
        filler_priority(b, retrieval_intent)
            .partial_cmp(&filler_priority(a, retrieval_intent))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for item in filler {
        if used.saturating_add(item.tokens) > budget {
            continue;
        }
        used = used.saturating_add(item.tokens);
        selected.push(item);
    }

    // ── Stage 3: build the pack ────────────────────────────────────
    let mut unknowns: Vec<String> = uncovered
        .iter()
        .map(|goal| format!("proof_gap: {}", goal.label()))
        .collect();
    if !uncovered.is_empty() {
        unknowns.insert(
            0,
            format!(
                "Minimal proof set exceeded {}-token budget; {} sub-goal(s) uncovered.",
                budget,
                uncovered.len()
            ),
        );
    }

    let context_pack = ContextPack {
        current_truth: filter_truth(&selected),
        project_facts: filter_by_type(&selected, MemoryType::Fact),
        recent_decisions: filter_by_type(&selected, MemoryType::Decision),
        active_tasks: filter_by_type(&selected, MemoryType::Task),
        constraints: filter_by_type(&selected, MemoryType::Constraint),
        known_bugs: filter_by_type(&selected, MemoryType::Bug),
        verified_fixes: filter_by_type(&selected, MemoryType::Fix),
        open_questions: filter_by_type(&selected, MemoryType::OpenQuestion),
        implementation_notes: filter_by_type(&selected, MemoryType::ImplementationDetail),
        repository_knowledge: filter_by_source(&selected, ContextSourceClass::Repository),
        session_continuity: filter_by_source(&selected, ContextSourceClass::SessionGraph),
        conflicts: filter_conflicts(&selected),
        proof_handles: unique_proof_handles(&selected),
        unknowns,
        recommended_verification: infer_recommended_verification(&selected, &uncovered),
        sources: unique_sources(&selected),
    };

    ContextBuildResponse {
        context_pack,
        token_usage: TokenUsage { budget, used },
        retrieval_intent,
    }
}

fn filler_priority(item: &ContextItem, retrieval_intent: RetrievalIntent) -> f64 {
    let source_weight = match item.source_class {
        ContextSourceClass::Repository => match retrieval_intent {
            RetrievalIntent::RepositoryOnly | RetrievalIntent::Hybrid => 0.8,
            _ => 0.3,
        },
        ContextSourceClass::Memory => 0.6,
        ContextSourceClass::SessionGraph => match retrieval_intent {
            RetrievalIntent::SessionGraphOnly | RetrievalIntent::Hybrid => 0.5,
            _ => 0.2,
        },
        ContextSourceClass::Conflict => 0.7,
    };
    source_weight * authority(item) * freshness(item)
}

fn filter_by_type(items: &[ContextItem], memory_type: MemoryType) -> Vec<ContextItem> {
    items
        .iter()
        .filter(|item| {
            item.source_class == ContextSourceClass::Memory && item.memory_type == memory_type
        })
        .cloned()
        .collect()
}

fn filter_by_source(items: &[ContextItem], source_class: ContextSourceClass) -> Vec<ContextItem> {
    items
        .iter()
        .filter(|item| item.source_class == source_class)
        .cloned()
        .collect()
}

fn filter_conflicts(items: &[ContextItem]) -> Vec<ContextItem> {
    items
        .iter()
        .filter(|item| {
            item.source_class == ContextSourceClass::Conflict
                || item.verification_status == Some(VerificationStatus::Contradicted)
        })
        .cloned()
        .collect()
}

fn filter_truth(items: &[ContextItem]) -> Vec<ContextItem> {
    items
        .iter()
        .filter(|item| match item.source_class {
            ContextSourceClass::Repository => true,
            ContextSourceClass::Memory => matches!(
                item.verification_status,
                Some(VerificationStatus::Verified | VerificationStatus::UserConfirmed)
            ),
            _ => false,
        })
        .cloned()
        .collect()
}

fn unique_proof_handles(items: &[ContextItem]) -> Vec<ProofHandle> {
    let mut keyed = BTreeMap::new();
    for item in items {
        for proof in &item.proof_handles {
            keyed.insert(
                (
                    proof.source_ref.clone(),
                    proof.session_id,
                    proof.session_event_id,
                    proof.proof_type,
                ),
                proof.clone(),
            );
        }
    }
    keyed.into_values().collect()
}

fn unique_sources(items: &[ContextItem]) -> Vec<chum_mem_contracts::ProvenanceHandle> {
    let mut keyed = BTreeMap::new();
    for item in items {
        for source in &item.provenance {
            keyed.insert((source.session_id, source.session_event_id), source.clone());
        }
    }
    keyed.into_values().collect()
}

fn infer_recommended_verification(
    selected: &[ContextItem],
    uncovered: &BTreeSet<SubGoal>,
) -> Vec<String> {
    let mut out = Vec::new();
    if !uncovered.is_empty() {
        out.push(
            "Compiled pack has uncovered sub-goals; narrow the objective or raise the budget."
                .to_string(),
        );
    }
    if selected
        .iter()
        .any(|item| item.verification_status == Some(VerificationStatus::Contradicted))
    {
        out.push(
            "Resolve contradicted claims with repository, tool, or test evidence.".to_string(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chum_mem_contracts::{AuthorityClass, MemoryType, VerificationStatus};

    fn item(
        memory_type: MemoryType,
        title: &str,
        tokens: u32,
        authority: AuthorityClass,
        verification: VerificationStatus,
    ) -> ContextItem {
        ContextItem {
            memory_id: None,
            reference_id: None,
            source_class: ContextSourceClass::Memory,
            ranking_role: None,
            memory_type,
            title: title.to_string(),
            summary: title.to_string(),
            tokens,
            provenance: Vec::new(),
            proof_handles: Vec::new(),
            claim_id: None,
            claim_key: Some(format!("{memory_type:?}:{title}")),
            claim_type: Some(memory_type),
            authority_class: Some(authority),
            verification_status: Some(verification),
            valid_from: None,
            valid_to: None,
            superseded_by: None,
        }
    }

    #[test]
    fn cover_selects_minimal_set() {
        let items = vec![
            item(
                MemoryType::Decision,
                "use tokio::select",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Task,
                "finish compiler",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Fact,
                "postgres 16 required",
                10,
                AuthorityClass::Repository,
                VerificationStatus::Verified,
            ),
            // Redundant decision — should NOT be picked over the cheaper one.
            item(
                MemoryType::Decision,
                "use tokio::select v2",
                100,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
        ];

        // Plain objective — sub-goal inference picks up only the core trio
        // (decisions, tasks, facts), matching the candidate set.
        let response = compile_minimal_proof_set(
            "resume prior work",
            &items,
            1000,
            RetrievalIntent::Hybrid,
        );

        assert!(
            response
                .context_pack
                .recent_decisions
                .iter()
                .any(|i| i.title == "use tokio::select")
        );
        assert_eq!(response.context_pack.active_tasks.len(), 1);
        assert_eq!(response.context_pack.project_facts.len(), 1);
        // No proof gap.
        assert!(
            !response
                .context_pack
                .unknowns
                .iter()
                .any(|u| u.starts_with("proof_gap:")),
            "unexpected proof gap: {:?}",
            response.context_pack.unknowns
        );
    }

    #[test]
    fn rejects_model_derived_candidate() {
        let items = vec![
            item(
                MemoryType::Decision,
                "model hallucination",
                10,
                AuthorityClass::ModelDerived,
                VerificationStatus::Inferred,
            ),
            item(
                MemoryType::Task,
                "active task",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
        ];

        let response = compile_minimal_proof_set(
            "continue work on retrieval",
            &items,
            1000,
            RetrievalIntent::Hybrid,
        );

        // Model-derived decision must not appear.
        assert!(
            response
                .context_pack
                .recent_decisions
                .is_empty(),
            "model-derived decision leaked into pack: {:?}",
            response.context_pack.recent_decisions
        );
        // Task still makes it in.
        assert_eq!(response.context_pack.active_tasks.len(), 1);
    }

    #[test]
    fn drops_superseded_claims() {
        let mut superseded = item(
            MemoryType::Decision,
            "old decision",
            10,
            AuthorityClass::UserConfirmed,
            VerificationStatus::UserConfirmed,
        );
        superseded.superseded_by = Some(uuid::Uuid::from_u128(1));

        let items = vec![
            superseded,
            item(
                MemoryType::Decision,
                "new decision",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
        ];

        let response = compile_minimal_proof_set(
            "continue work",
            &items,
            1000,
            RetrievalIntent::Hybrid,
        );

        assert_eq!(response.context_pack.recent_decisions.len(), 1);
        assert_eq!(
            response.context_pack.recent_decisions[0].title,
            "new decision"
        );
    }

    #[test]
    fn emits_proof_gap_on_budget_overflow() {
        // Every candidate costs more than the budget — compiler cannot
        // cover any sub-goal and must surface the gap.
        let items = vec![
            item(
                MemoryType::Decision,
                "expensive decision",
                5_000,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Task,
                "expensive task",
                5_000,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Fact,
                "expensive fact",
                5_000,
                AuthorityClass::Repository,
                VerificationStatus::Verified,
            ),
        ];

        let response = compile_minimal_proof_set(
            "resume retrieval work",
            &items,
            1_000, // hard ceiling below any single candidate
            RetrievalIntent::Hybrid,
        );

        // All three core sub-goals should be uncovered.
        let proof_gaps: Vec<_> = response
            .context_pack
            .unknowns
            .iter()
            .filter(|u| u.starts_with("proof_gap:"))
            .collect();
        assert_eq!(proof_gaps.len(), 3, "expected 3 gaps, got {proof_gaps:?}");
        // Nothing was selected.
        assert_eq!(response.token_usage.used, 0);
    }

    #[test]
    fn sub_goal_inference_expands_on_keywords() {
        let goals = infer_sub_goals("fix the bug in the ranker");
        assert!(goals.contains(&SubGoal::Bugs));
        assert!(goals.contains(&SubGoal::Fixes));

        let goals2 = infer_sub_goals("refactor the context module");
        assert!(goals2.contains(&SubGoal::Implementation));
    }

    // ── v2.2.3: Section-aware fill tests ──────────────────────────

    #[test]
    fn core_trio_always_present_in_sub_goals() {
        let goals = infer_sub_goals("do something generic");
        assert!(goals.contains(&SubGoal::Decisions));
        assert!(goals.contains(&SubGoal::ActiveTasks));
        assert!(goals.contains(&SubGoal::Facts));
    }

    #[test]
    fn section_fill_populates_all_typed_sections() {
        let items = vec![
            item(
                MemoryType::Decision,
                "use Rust",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Task,
                "finish tests",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Fact,
                "postgres 16",
                10,
                AuthorityClass::Repository,
                VerificationStatus::Verified,
            ),
            item(
                MemoryType::Bug,
                "ranking drift",
                10,
                AuthorityClass::ToolVerified,
                VerificationStatus::Verified,
            ),
            item(
                MemoryType::Fix,
                "fixed ranking",
                10,
                AuthorityClass::TestVerified,
                VerificationStatus::Verified,
            ),
            item(
                MemoryType::Constraint,
                "no model-derived",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::OpenQuestion,
                "how to scale",
                10,
                AuthorityClass::SessionDerived,
                VerificationStatus::Verified,
            ),
            item(
                MemoryType::ImplementationDetail,
                "weighted set-cover",
                10,
                AuthorityClass::Repository,
                VerificationStatus::Verified,
            ),
        ];

        let response = compile_minimal_proof_set(
            "debug the bug in the implementation and answer open questions about constraints",
            &items,
            10_000,
            RetrievalIntent::Hybrid,
        );

        let pack = &response.context_pack;
        assert!(!pack.recent_decisions.is_empty(), "decisions should be filled");
        assert!(!pack.active_tasks.is_empty(), "tasks should be filled");
        assert!(!pack.project_facts.is_empty(), "facts should be filled");
        assert!(!pack.known_bugs.is_empty(), "bugs should be filled");
        assert!(!pack.verified_fixes.is_empty(), "fixes should be filled");
        assert!(!pack.constraints.is_empty(), "constraints should be filled");
        assert!(!pack.open_questions.is_empty(), "open_questions should be filled");
        assert!(!pack.implementation_notes.is_empty(), "implementation should be filled");
        assert!(
            pack.unknowns.iter().all(|u| !u.starts_with("proof_gap:")),
            "no proof gaps expected: {:?}",
            pack.unknowns
        );
    }

    #[test]
    fn proof_gap_emitted_for_missing_bug_section() {
        let items = vec![
            item(
                MemoryType::Decision,
                "use Rust",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Task,
                "finish work",
                10,
                AuthorityClass::UserConfirmed,
                VerificationStatus::UserConfirmed,
            ),
            item(
                MemoryType::Fact,
                "postgres 16",
                10,
                AuthorityClass::Repository,
                VerificationStatus::Verified,
            ),
        ];

        let response = compile_minimal_proof_set(
            "debug the bug in the ranker",
            &items,
            10_000,
            RetrievalIntent::Hybrid,
        );

        let gaps: Vec<_> = response
            .context_pack
            .unknowns
            .iter()
            .filter(|u| u.starts_with("proof_gap:"))
            .collect();
        assert!(
            gaps.iter().any(|g| g.contains("bugs")),
            "should emit proof_gap for missing bugs section: {gaps:?}"
        );
        assert!(
            gaps.iter().any(|g| g.contains("fixes")),
            "should emit proof_gap for missing fixes section: {gaps:?}"
        );
    }

    // ── Supersession correctness in compilation ───────────────────

    #[test]
    fn superseded_claims_excluded_from_cover() {
        let mut old = item(
            MemoryType::Fact,
            "old fact",
            10,
            AuthorityClass::Repository,
            VerificationStatus::Verified,
        );
        old.superseded_by = Some(uuid::Uuid::from_u128(42));

        let current = item(
            MemoryType::Fact,
            "current fact",
            10,
            AuthorityClass::Repository,
            VerificationStatus::Verified,
        );

        let items = vec![old, current];
        let response = compile_minimal_proof_set(
            "get the facts",
            &items,
            1000,
            RetrievalIntent::Hybrid,
        );

        assert_eq!(response.context_pack.project_facts.len(), 1);
        assert_eq!(
            response.context_pack.project_facts[0].title,
            "current fact"
        );
    }

    #[test]
    fn contradicted_claims_excluded_from_cover() {
        let contradicted = item(
            MemoryType::Decision,
            "contradicted approach",
            10,
            AuthorityClass::UserConfirmed,
            VerificationStatus::Contradicted,
        );
        let valid = item(
            MemoryType::Decision,
            "valid approach",
            10,
            AuthorityClass::UserConfirmed,
            VerificationStatus::UserConfirmed,
        );

        let items = vec![contradicted, valid];
        let response = compile_minimal_proof_set(
            "resume work",
            &items,
            1000,
            RetrievalIntent::Hybrid,
        );

        assert_eq!(response.context_pack.recent_decisions.len(), 1);
        assert_eq!(
            response.context_pack.recent_decisions[0].title,
            "valid approach"
        );
    }
}
