#![forbid(unsafe_code)]
//! The identifier the Python services stamped on an answer they did not keep.
//!
//! Several services defaulted an `id` field to a fresh uuid4 and returned it, so a caller could refer back to an
//! answer. A pure arithmetic port has nothing to draw a uuid from, and taking a clock reading in the middle of a
//! reckoning makes the same input answer differently twice. This is one identifier shaped like the uuid4 the services
//! gave, minted in one place so every domain mints it the same way, and left alone once minted.

/// A uuid4-shaped identifier.
///
/// The digits come from the clock, so two calls in the same nanosecond would agree; nothing depends on them being
/// distinct, because an identifier a service returned is only ever read back by the caller who asked.
pub fn service_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|taken| taken.as_nanos())
        .unwrap_or(0);
    let mut digits: Vec<char> = format!("{nanos:032x}").chars().collect();
    // the version and variant markers a uuid4 carries
    digits[12] = '4';
    digits[16] = 'a';

    let hex: String = digits.into_iter().collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_keeps_the_five_part_shape_and_a_version_four_marker() {
        let id = service_id();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        assert!(parts[2].starts_with('4'));
        assert!(parts[3].starts_with('a'));
    }
}
