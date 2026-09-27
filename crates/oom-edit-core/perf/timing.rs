//! Deterministic timing-oracle helpers shared by release and smoke gates.

use std::time::Duration;

pub(crate) fn duration_gate_passes(observed: Duration, target: Duration) -> bool {
    observed < target
}

pub(crate) fn checked_additional_duration(
    baseline: Duration,
    with_feature: Duration,
) -> Option<Duration> {
    if baseline.is_zero() || with_feature.is_zero() {
        None
    } else {
        Some(with_feature.saturating_sub(baseline))
    }
}
