//! Audit assurance assessments: evidence, going concern, governance and due diligence.
//!
//! Each module records what the Python service decided and why a choice might look odd, so a
//! reviewer can tell a deliberate simplification from a defect.

#![forbid(unsafe_code)]

pub mod due_diligence;
pub mod evidence;
pub mod going_concern;
pub mod governance;

/// `audit-evidence-service /evaluate`.
pub fn audit_evidence_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    evidence::evaluate(payload)
}

/// `going-concern-service /assess`.
pub fn going_concern_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    going_concern::assess(payload)
}

/// `corporate-governance-service /assess`.
pub fn corporate_governance_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    governance::assess(payload)
}

/// `due-diligence-service /assess`.
pub fn due_diligence_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    due_diligence::assess(payload)
}
