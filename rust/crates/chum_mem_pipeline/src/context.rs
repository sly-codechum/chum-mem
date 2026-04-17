use std::collections::{BTreeMap, BTreeSet};

use chum_mem_contracts::{
    AuthorityClass, ContextBuildResponse, ContextItem, ContextPack, ContextSourceClass, MemoryType,
    ProofHandle, RetrievalIntent, TokenUsage, VerificationStatus,
};

pub fn build_context_pack(
    items: &[ContextItem],
    budget: u32,
    retrieval_intent: RetrievalIntent,
) -> ContextBuildResponse {
    let mut candidates = items.to_vec();
    candidates.sort_by(|left, right| {
        context_priority(right, retrieval_intent)
            .cmp(&context_priority(left, retrieval_intent))
            .then_with(|| right.tokens.cmp(&left.tokens))
    });

    let mut selected = Vec::new();
    let mut used = 0_u32;
    let mut seen = BTreeSet::new();
    let mut per_bucket = BTreeMap::<String, usize>::new();

    for item in candidates {
        let dedupe_key = item.claim_key.clone().unwrap_or_else(|| {
            format!(
                "{:?}:{}:{}",
                item.source_class,
                item.title.trim().to_lowercase(),
                item.summary.trim().to_lowercase()
            )
        });
        if !seen.insert(dedupe_key) {
            continue;
        }
        let bucket = bucket_name(&item);
        let quota = bucket_quota(&bucket, retrieval_intent);
        let count = per_bucket.entry(bucket.clone()).or_default();
        if *count >= quota {
            continue;
        }
        if used.saturating_add(item.tokens) > budget {
            continue;
        }
        used = used.saturating_add(item.tokens);
        *count += 1;
        selected.push(item);
    }

    let context_pack = ContextPack {
        current_truth: current_truth_items(&selected),
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
        conflicts: conflict_items(&selected),
        proof_handles: unique_proof_handles(&selected),
        unknowns: infer_unknowns(&selected, retrieval_intent),
        recommended_verification: infer_recommended_verification(&selected, retrieval_intent),
        sources: unique_sources(&selected),
    };

    ContextBuildResponse {
        context_pack,
        token_usage: TokenUsage { budget, used },
        retrieval_intent,
    }
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

fn conflict_items(items: &[ContextItem]) -> Vec<ContextItem> {
    items
        .iter()
        .filter(|item| {
            item.source_class == ContextSourceClass::Conflict
                || item.verification_status == Some(VerificationStatus::Contradicted)
        })
        .cloned()
        .collect()
}

fn current_truth_items(items: &[ContextItem]) -> Vec<ContextItem> {
    items
        .iter()
        .filter(|item| match item.source_class {
            ContextSourceClass::Repository => true,
            ContextSourceClass::Memory => is_truth_claim(item),
            ContextSourceClass::SessionGraph | ContextSourceClass::Conflict => false,
        })
        .cloned()
        .collect()
}

fn is_truth_claim(item: &ContextItem) -> bool {
    matches!(
        item.verification_status,
        Some(VerificationStatus::Verified | VerificationStatus::UserConfirmed)
    ) || matches!(
        item.authority_class,
        Some(
            AuthorityClass::Repository
                | AuthorityClass::ToolVerified
                | AuthorityClass::TestVerified
                | AuthorityClass::UserConfirmed
        )
    )
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

fn bucket_name(item: &ContextItem) -> String {
    match item.source_class {
        ContextSourceClass::Repository => "repository".to_string(),
        ContextSourceClass::SessionGraph => "session_graph".to_string(),
        ContextSourceClass::Conflict => "conflict".to_string(),
        ContextSourceClass::Memory => format!("memory:{:?}", item.memory_type),
    }
}

fn bucket_quota(bucket: &str, retrieval_intent: RetrievalIntent) -> usize {
    match retrieval_intent {
        RetrievalIntent::None => 0,
        RetrievalIntent::RepositoryOnly => match bucket {
            "repository" => 6,
            "conflict" => 2,
            _ => 2,
        },
        RetrievalIntent::MemoryOnly => match bucket {
            "session_graph" => 2,
            "conflict" => 2,
            _ => 3,
        },
        RetrievalIntent::SessionGraphOnly => match bucket {
            "session_graph" => 6,
            "conflict" => 2,
            _ => 0,
        },
        RetrievalIntent::Hybrid => match bucket {
            "repository" => 4,
            "session_graph" => 2,
            "conflict" => 2,
            _ => 2,
        },
    }
}

fn context_priority(item: &ContextItem, retrieval_intent: RetrievalIntent) -> i32 {
    let source_weight = match item.source_class {
        ContextSourceClass::Repository => match retrieval_intent {
            RetrievalIntent::RepositoryOnly => 70,
            RetrievalIntent::Hybrid => 60,
            _ => 30,
        },
        ContextSourceClass::Memory => match retrieval_intent {
            RetrievalIntent::MemoryOnly => 60,
            RetrievalIntent::Hybrid => 50,
            _ => 25,
        },
        ContextSourceClass::SessionGraph => match retrieval_intent {
            RetrievalIntent::SessionGraphOnly => 55,
            RetrievalIntent::Hybrid | RetrievalIntent::MemoryOnly => 30,
            _ => 10,
        },
        ContextSourceClass::Conflict => 45,
    };
    let verification_weight = match item.verification_status {
        Some(VerificationStatus::Verified) => 25,
        Some(VerificationStatus::UserConfirmed) => 20,
        Some(VerificationStatus::Inferred) => 8,
        Some(VerificationStatus::Unverified) => -4,
        Some(VerificationStatus::Contradicted) => -15,
        None => 0,
    };
    let authority_weight = match item.authority_class {
        Some(AuthorityClass::Repository) => 20,
        Some(AuthorityClass::TestVerified) => 18,
        Some(AuthorityClass::ToolVerified) => 16,
        Some(AuthorityClass::UserConfirmed) => 14,
        Some(AuthorityClass::SessionDerived) => 4,
        Some(AuthorityClass::ModelDerived) => -10,
        None => 0,
    };
    let type_weight = match item.memory_type {
        MemoryType::Decision | MemoryType::Fact | MemoryType::Constraint | MemoryType::Fix => 14,
        MemoryType::Task | MemoryType::Bug => 10,
        MemoryType::ImplementationDetail => 8,
        MemoryType::OpenQuestion => 6,
        MemoryType::Summary | MemoryType::Risk | MemoryType::ChangeLog => -6,
    };
    source_weight + verification_weight + authority_weight + type_weight
}

fn infer_unknowns(items: &[ContextItem], retrieval_intent: RetrievalIntent) -> Vec<String> {
    let has_repository = items
        .iter()
        .any(|item| item.source_class == ContextSourceClass::Repository);
    let has_truth_claims = items
        .iter()
        .any(|item| item.source_class == ContextSourceClass::Memory && is_truth_claim(item));
    let has_conflicts = items.iter().any(|item| {
        item.source_class == ContextSourceClass::Conflict
            || item.verification_status == Some(VerificationStatus::Contradicted)
    });

    let mut unknowns = Vec::new();
    match retrieval_intent {
        RetrievalIntent::RepositoryOnly if !has_repository => unknowns.push(
            "Repository knowledge was requested but no repository proof was retrieved.".to_string(),
        ),
        RetrievalIntent::MemoryOnly if !has_truth_claims => unknowns.push(
            "Continuity was requested but only low-confidence or summary-style memory was available."
                .to_string(),
        ),
        RetrievalIntent::Hybrid if !has_repository || !has_truth_claims => unknowns.push(
            "Hybrid retrieval did not produce both repository proof and verified continuity claims."
                .to_string(),
        ),
        RetrievalIntent::SessionGraphOnly if items.is_empty() => unknowns.push(
            "Session history was requested but the session graph had no matching evidence."
                .to_string(),
        ),
        _ => {}
    }
    if has_conflicts {
        unknowns.push("Conflicting claims were retrieved and require verification.".to_string());
    }
    unknowns
}

fn infer_recommended_verification(
    items: &[ContextItem],
    retrieval_intent: RetrievalIntent,
) -> Vec<String> {
    let mut recommendations = Vec::new();
    if items.iter().any(|item| {
        item.verification_status == Some(VerificationStatus::Contradicted)
            || item.source_class == ContextSourceClass::Conflict
    }) {
        recommendations.push(
            "Resolve conflicting claims with the latest repository, tool, or test evidence."
                .to_string(),
        );
    }
    if matches!(
        retrieval_intent,
        RetrievalIntent::MemoryOnly | RetrievalIntent::Hybrid
    ) && !items.iter().any(is_truth_claim)
    {
        recommendations.push(
            "Promote explicit decisions, tasks, constraints, and verified fixes into claim memory instead of relying on summaries."
                .to_string(),
        );
    }
    if matches!(
        retrieval_intent,
        RetrievalIntent::RepositoryOnly | RetrievalIntent::Hybrid
    ) && !items
        .iter()
        .any(|item| item.source_class == ContextSourceClass::Repository)
    {
        recommendations
            .push("Refresh repository sync or query repository knowledge directly.".to_string());
    }
    recommendations
}
