//! Time-based one-time passwords (RFC 6238), as authenticator apps speak them.
//!
//! Why this exists: the engine raised its own six-digit codes for a second factor, and had nowhere to send them, so a
//! person who switched the second factor on could never finish a sign-in. The code existed only in an audit record they
//! cannot read. An authenticator app is the one delivery channel that needs no mail server and no phone number, which makes
//! this the shape the second factor has to take to be real on a device.
//!
//! The parameters are the interoperable defaults on purpose - SHA-1, six digits, thirty seconds, a 160-bit secret - because
//! that is what Google Authenticator, Authy, 1Password, Bitwarden and the rest agree on. A shorter code or a newer hash would
//! be defensible on its own and would still be the wrong choice: a second factor nobody's app accepts is a lockout.
//!
//! Verification accepts exactly one step of drift either way - a phone clock a few seconds off, or a person reading the code
//! as it turns over - and reports the step it matched, so a caller can refuse a code it has already spent. Without that, a
//! code stays usable for its whole thirty seconds after the first successful use, which on a shared screen is a second way in.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use subtle::ConstantTimeEq;

type HmacSha1 = Hmac<Sha1>;

/// Digits in a code. Six is what every authenticator app offers by default.
pub const DIGITS: u32 = 6;

/// How long one code lasts, in seconds (RFC 6238's default step).
pub const STEP_SECONDS: u64 = 30;

/// Steps of drift tolerated either side, i.e. one code before or after the current one.
pub const DRIFT_STEPS: u64 = 1;

/// Bytes in a fresh secret: 160 bits, the size RFC 4226 recommends for HMAC-SHA1.
pub const SECRET_BYTES: usize = 20;

/// The issuer name shown beside an entry in an authenticator app.
pub const ISSUER: &str = "Vimbai";

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// A fresh secret, encoded the way authenticator apps expect to be given one.
///
/// The randomness comes from the operating system: a secret a guess could reach is not a second factor.
pub fn secret() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    getrandom::getrandom(&mut bytes).expect("the operating system provides entropy");
    encode_base32(&bytes)
}

/// Base32 (RFC 4648) with no padding, the form authenticator apps read.
pub fn encode_base32(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut held: u32 = 0;
    let mut bits = 0u32;
    for byte in bytes {
        held = (held << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = ((held >> bits) & 0x1f) as usize;
            text.push(ALPHABET[index] as char);
        }
    }
    if bits > 0 {
        let index = ((held << (5 - bits)) & 0x1f) as usize;
        text.push(ALPHABET[index] as char);
    }
    text
}

/// Read a secret a person typed or pasted.
///
/// Lower case, spaces, dashes and padding are all accepted and thrown away: people copy these out of a screen, and a secret
/// refused for its spacing is a person locked out of their own account.
pub fn decode_base32(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len() * 5 / 8);
    let mut held: u32 = 0;
    let mut bits = 0u32;
    for character in text.chars() {
        if matches!(character, ' ' | '-' | '=' | '\n' | '\r' | '\t') {
            continue;
        }
        let upper = character.to_ascii_uppercase();
        let value = match upper {
            'A'..='Z' => upper as u32 - 'A' as u32,
            '2'..='7' => upper as u32 - '2' as u32 + 26,
            _ => return None,
        };
        held = (held << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((held >> bits) & 0xff) as u8);
        }
    }
    if bytes.is_empty() {
        return None;
    }
    Some(bytes)
}

/// The RFC 4226 value for one counter, before it is shortened to a code.
///
/// Exposed so the published test vectors can be checked against it directly, rather than only through a six-digit result that
/// would hide a truncation mistake.
pub fn hotp_value(secret_b32: &str, counter: u64) -> Option<u32> {
    let key = decode_base32(secret_b32)?;
    let mut mac = HmacSha1::new_from_slice(&key).ok()?;
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    // RFC 4226 dynamic truncation: the low nibble of the last byte picks the four bytes to use
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let slice = digest.get(offset..offset + 4)?;
    let value = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) & 0x7fff_ffff;
    Some(value)
}

/// The step a moment in time belongs to.
pub fn step_at(unix_seconds: u64) -> u64 {
    unix_seconds / STEP_SECONDS
}

/// The code an authenticator app shows at a moment in time.
pub fn code_at(secret_b32: &str, unix_seconds: u64) -> Option<String> {
    let value = hotp_value(secret_b32, step_at(unix_seconds))?;
    Some(format!(
        "{:0width$}",
        value % 10u32.pow(DIGITS),
        width = DIGITS as usize
    ))
}

/// Check a code, returning the step it matched when it is good.
///
/// The comparison is constant time and every candidate step is always tried, so the answer says nothing about how close a
/// wrong code was. The step that matched is returned so a caller can refuse a code it has already spent: within one code's
/// life a second use is the same password, and must not be a second way in.
pub fn verify_at(secret_b32: &str, code: &str, unix_seconds: u64, drift: u64) -> Option<u64> {
    let wanted = code.trim();
    if wanted.len() != DIGITS as usize || !wanted.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let step = step_at(unix_seconds);
    let first = step.saturating_sub(drift);
    let last = step.saturating_add(drift);
    let mut found: Option<u64> = None;
    for candidate in first..=last {
        let value = hotp_value(secret_b32, candidate)?;
        let shown = format!(
            "{:0width$}",
            value % 10u32.pow(DIGITS),
            width = DIGITS as usize
        );
        // constant time, and no early exit: a mismatch at the first digit costs the same as one at the last
        let equal: bool = shown.as_bytes().ct_eq(wanted.as_bytes()).into();
        if equal && found.is_none() {
            found = Some(candidate);
        }
    }
    found
}

/// The `otpauth://` address an authenticator app reads from a QR code.
///
/// The app also shows this as text, because a phone that is showing the QR code cannot scan it: the person types it into their
/// authenticator's "enter a key manually" screen, or copies the secret itself.
pub fn provisioning_uri(account: &str, secret_b32: &str) -> String {
    let label = format!("{}:{}", ISSUER, account);
    format!(
        "otpauth://totp/{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        percent_encode(&label),
        secret_b32,
        percent_encode(ISSUER),
        DIGITS,
        STEP_SECONDS
    )
}

/// Percent-encode the parts of the address that are not plain unreserved characters.
fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        let plain =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b':');
        if plain {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The secret RFC 6238 uses throughout its own vectors: "12345678901234567890".
    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn base32_round_trips_and_forgives_what_people_paste() {
        let encoded = encode_base32(b"12345678901234567890");
        assert_eq!(encoded, RFC_SECRET);
        assert_eq!(decode_base32(RFC_SECRET).unwrap(), b"12345678901234567890");
        let spaced = RFC_SECRET
            .chars()
            .collect::<Vec<_>>()
            .chunks(4)
            .map(|group| group.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            decode_base32(&spaced.to_lowercase()).unwrap(),
            b"12345678901234567890",
            "a secret typed in groups and lower case is the same secret"
        );
        assert!(decode_base32("not base32!").is_none());
        assert!(decode_base32("").is_none());
    }

    #[test]
    fn the_published_vectors_hold() {
        // RFC 6238, appendix B, SHA-1, eight digits: the published code is the 31-bit value shortened to eight digits
        for (unix_time, expected) in [
            (59u64, 94_287_082u32),
            (1_111_111_109, 7_081_804),
            (1_111_111_111, 14_050_471),
            (1_234_567_890, 89_005_924),
            (2_000_000_000, 69_279_037),
            (20_000_000_000, 65_353_130),
        ] {
            let value = hotp_value(RFC_SECRET, step_at(unix_time)).expect("a value");
            assert_eq!(
                value % 100_000_000,
                expected,
                "RFC 6238 vector at {unix_time}"
            );
        }
        assert_eq!(code_at(RFC_SECRET, 59).unwrap(), "287082");
    }

    #[test]
    fn a_code_is_good_within_one_step_and_no_further() {
        let at = 1_700_000_000u64;
        let code = code_at(RFC_SECRET, at).unwrap();
        assert_eq!(
            verify_at(RFC_SECRET, &code, at, DRIFT_STEPS),
            Some(step_at(at))
        );
        // a phone thirty seconds behind, and one thirty seconds ahead
        assert!(verify_at(RFC_SECRET, &code, at - 25, DRIFT_STEPS).is_some());
        assert!(verify_at(RFC_SECRET, &code, at + 25, DRIFT_STEPS).is_some());
        // a minute out is refused: drift is a clock's excuse, not an unlocked window
        assert!(verify_at(RFC_SECRET, &code, at + 65, DRIFT_STEPS).is_none());
        assert!(verify_at(RFC_SECRET, &code, at - 65, DRIFT_STEPS).is_none());
    }

    #[test]
    fn wrong_shapes_are_refused_without_asking_the_secret() {
        let at = 1_700_000_000u64;
        for wrong in ["", "12345", "1234567", "abcdef", " 12345 ", "12345a"] {
            assert!(
                verify_at(RFC_SECRET, wrong, at, DRIFT_STEPS).is_none(),
                "{wrong:?} is not a code"
            );
        }
        assert!(verify_at("!!!!", "123456", at, DRIFT_STEPS).is_none());
    }

    #[test]
    fn the_matched_step_is_reported_so_a_use_can_be_spent() {
        let at = 1_700_000_000u64;
        let code = code_at(RFC_SECRET, at).unwrap();
        let matched = verify_at(RFC_SECRET, &code, at, DRIFT_STEPS).expect("the code is good");
        assert_eq!(matched, step_at(at));
        // the same code at the same moment matches the same step, which is what lets a caller refuse the second use
        assert_eq!(
            verify_at(RFC_SECRET, &code, at + 5, DRIFT_STEPS),
            Some(matched)
        );
    }

    #[test]
    fn a_fresh_secret_is_random_and_usable() {
        let first = secret();
        let second = secret();
        assert_ne!(first, second, "two enrolments must not share a secret");
        assert_eq!(decode_base32(&first).unwrap().len(), SECRET_BYTES);
        let at = 1_700_000_000u64;
        let code = code_at(&first, at).unwrap();
        assert_eq!(code.len(), DIGITS as usize);
        assert!(verify_at(&first, &code, at, DRIFT_STEPS).is_some());
        assert!(
            verify_at(&second, &code, at, DRIFT_STEPS).is_none(),
            "another account's secret must not accept this code"
        );
    }

    #[test]
    fn the_provisioning_address_names_the_app_and_carries_the_defaults() {
        let uri = provisioning_uri("sam@example.com", RFC_SECRET);
        assert!(uri.starts_with("otpauth://totp/Vimbai:sam%40example.com?"));
        assert!(uri.contains(&format!("secret={RFC_SECRET}")));
        assert!(uri.contains("issuer=Vimbai"));
        assert!(uri.contains("algorithm=SHA1"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }
}
