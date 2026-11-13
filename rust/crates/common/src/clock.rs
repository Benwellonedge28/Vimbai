#![forbid(unsafe_code)]
//! A clock reading for the moments a service must stamp when the caller states none.
//!
//! Most ports take "now" from the request, because a reckoning that reads the clock answers differently twice and cannot
//! be replayed. Some records, though, are only useful with the real moment they were made - a pipeline's run, a load's
//! start - and leaving those null would lose the one fact they exist to carry. This is the single place that reading
//! comes from, so a stamped moment is the same shape everywhere and the clock is only consulted when nobody said.

/// Seconds since the epoch, as the services stamped them.
pub fn moment() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_is_a_plausible_moment_and_never_runs_backwards() {
        let taken = moment();
        // after 2020 and before 2100: a real clock, not a placeholder
        assert!(
            (1_577_836_800.0..4_102_444_800.0).contains(&taken),
            "{taken}"
        );
        assert!(moment() >= taken);
    }
}
