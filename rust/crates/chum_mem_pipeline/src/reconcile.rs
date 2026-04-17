//! PCKC claim reconciliation policy.
//!
//! Pure functions extracted from the API's `derive_and_persist_session_memories`
//! monolith so the async `reconcile-claim-state` worker job can reuse the same
//! supersession/contradiction rules without pulling in api/main.rs.

use chum_mem_contracts::{AuthorityClass, MemoryType, VerificationStatus};

/// Returns true when `current` should supersede `prior` according to the PCKC
/// policy table: (memory_type pair, strength comparison).
///
/// Keep in sync with the original implementation at
/// `rust/apps/api/src/main.rs::current_supersedes_prior` (pre v2.2.1 fix).
pub fn current_supersedes_prior(
    current_memory_type: MemoryType,
    prior_memory_type: MemoryType,
    current_verification: Option<VerificationStatus>,
    prior_verification: Option<VerificationStatus>,
    current_authority: Option<AuthorityClass>,
    prior_authority: Option<AuthorityClass>,
) -> bool {
    let current_stronger = claim_strength(current_authority, current_verification)
        >= claim_strength(prior_authority, prior_verification);
    matches!(
        (current_memory_type, prior_memory_type),
        (MemoryType::Fix, MemoryType::Bug)
            | (MemoryType::Decision, MemoryType::Decision)
            | (MemoryType::Constraint, MemoryType::Constraint)
            | (MemoryType::Constraint, MemoryType::Decision)
            | (MemoryType::Fact, MemoryType::Fact)
            | (MemoryType::Task, MemoryType::Task)
    ) && current_stronger
}

/// Composite strength score used to compare two claims.
pub fn claim_strength(
    authority_class: Option<AuthorityClass>,
    verification_status: Option<VerificationStatus>,
) -> i32 {
    let authority_score = match authority_class {
        Some(AuthorityClass::Repository) => 50,
        Some(AuthorityClass::TestVerified) => 45,
        Some(AuthorityClass::ToolVerified) => 40,
        Some(AuthorityClass::UserConfirmed) => 35,
        Some(AuthorityClass::SessionDerived) => 20,
        Some(AuthorityClass::ModelDerived) => 0,
        None => 10,
    };
    authority_score + verification_rank(verification_status)
}

/// Ordinal rank for the verification axis.
pub fn verification_rank(status: Option<VerificationStatus>) -> i32 {
    match status {
        Some(VerificationStatus::Verified) => 4,
        Some(VerificationStatus::UserConfirmed) => 3,
        Some(VerificationStatus::Inferred) => 2,
        Some(VerificationStatus::Unverified) => 1,
        Some(VerificationStatus::Contradicted) => 0,
        None => 1,
    }
}

pub fn parse_authority_class(value: &str) -> Option<AuthorityClass> {
    match value {
        "repository" => Some(AuthorityClass::Repository),
        "test_verified" => Some(AuthorityClass::TestVerified),
        "tool_verified" => Some(AuthorityClass::ToolVerified),
        "user_confirmed" => Some(AuthorityClass::UserConfirmed),
        "session_derived" => Some(AuthorityClass::SessionDerived),
        "model_derived" => Some(AuthorityClass::ModelDerived),
        _ => None,
    }
}

pub fn parse_verification_status(value: &str) -> Option<VerificationStatus> {
    match value {
        "verified" => Some(VerificationStatus::Verified),
        "user_confirmed" => Some(VerificationStatus::UserConfirmed),
        "inferred" => Some(VerificationStatus::Inferred),
        "unverified" => Some(VerificationStatus::Unverified),
        "contradicted" => Some(VerificationStatus::Contradicted),
        _ => None,
    }
}

pub fn parse_memory_type(value: &str) -> MemoryType {
    match value {
        "fact" => MemoryType::Fact,
        "decision" => MemoryType::Decision,
        "task" => MemoryType::Task,
        "bug" => MemoryType::Bug,
        "summary" => MemoryType::Summary,
        "implementation_detail" => MemoryType::ImplementationDetail,
        "change_log" => MemoryType::ChangeLog,
        "risk" => MemoryType::Risk,
        "constraint" => MemoryType::Constraint,
        "fix" => MemoryType::Fix,
        "open_question" => MemoryType::OpenQuestion,
        _ => MemoryType::Fact,
    }
}
