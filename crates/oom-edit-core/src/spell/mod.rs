#[allow(
    dead_code,
    reason = "the complete Phase 3 scanner is exercised directly before SpellState consumes it"
)]
mod exclusions;

pub(crate) fn normalize_reference_label(label: &str) -> Option<String> {
    let label = label
        .strip_prefix('[')
        .and_then(|label| label.strip_suffix(']'))
        .unwrap_or(label);
    if label.is_empty() || label.len() > 999 {
        return None;
    }
    let normalized: String = label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}
