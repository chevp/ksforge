use crate::capability::CapabilityRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);

/// Deterministic, technical validation — no LLM involved. An intelligence
/// layer may add semantic validation on top of this later; it can never
/// replace it (section 16).
pub fn validate_capability_request(request: &CapabilityRequest) -> Result<(), ValidationError> {
    if request.payload.is_empty() {
        return Err(ValidationError("payload must not be empty".to_string()));
    }
    Ok(())
}
