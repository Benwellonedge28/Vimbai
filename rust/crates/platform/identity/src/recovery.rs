//! Recovery codes: the way back in when the phone that holds the second factor is gone.
//!
//! Why this exists: the second factor is real - an authenticator app on one device - and a real second factor has a real
//! failure mode. Lose or replace the phone and every code in the world is the wrong code, so without a way back in the second
//! factor is not security, it is a lockout waiting for a bad day. Recovery codes are the standard answer: a short list of
//! single-use codes handed over once, when the factor is turned on, and each good for one sign-in in place of a code from the
//! app.
//!
//! What makes them different from the six digits: they are long enough to be unguessable outright (fifty bits each), they are
//! shown once and stored only as hashes, and each one dies the moment it is used. A six-digit code is guessable inside its
//! thirty seconds, which is why it is rate-limited and why it expires; a recovery code is not guessable at all, which is why it
//! can sit in a drawer for a year and still be worth exactly one sign-in.
//!
//! Grouping and alphabet: ten characters in five-letter groups, from the Crockford base32 alphabet - no I, L, O or U - because
//! these are read off a screen, copied by hand onto paper, and typed back months later. A code refused for its lettering is a
//! person locked out with the right code in their hand.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Characters in one code, before grouping.
pub const DIGITS: usize = 10;

/// How many codes are issued at once. Ten is what the field settled on: enough to survive a few lost printouts, few enough
/// that a person will actually store them.
pub const COUNT: usize = 10;

/// The alphabet a code is written in: Crockford base32, which leaves out the letters that are misread as digits.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Characters in one group, for legibility on a screen and on paper.
const GROUP: usize = 5;

/// A fresh set of recovery codes, in the two forms a caller needs: the text to show once, and the hashes to store.
///
/// The plaintext is returned only here and never stored, exactly as with a password: what is kept is the hash, so a stolen
/// database cannot be used to sign in. The two lists are in the same order, so a caller may keep the plaintext beside the
/// hash for the length of one response and no longer.
pub fn issue(person: &str) -> (Vec<String>, Vec<String>) {
    let mut shown = Vec::with_capacity(COUNT);
    let mut held = Vec::with_capacity(COUNT);
    while shown.len() < COUNT {
        let Some(one) = generate() else { continue };
        // a repeat inside one set is a wasted code, so it is drawn again rather than shown twice
        if shown.contains(&one) {
            continue;
        }
        held.push(hash_for(person, &one));
        shown.push(one);
    }
    (shown, held)
}

/// One code, written the way it is shown: five characters, a dash, five more.
fn generate() -> Option<String> {
    let mut bytes = [0u8; DIGITS];
    getrandom::getrandom(&mut bytes).ok()?;
    // 256 is not a multiple of 32, so a byte is folded onto the alphabet by taking it modulo 32 - every character of the
    // alphabet is reachable, and the twentieth-of-a-bit of bias that costs is nothing next to the fifty bits of entropy
    // the code already carries
    let mut text = String::with_capacity(DIGITS + 1);
    for (index, byte) in bytes.iter().enumerate() {
        if index == GROUP {
            text.push('-');
        }
        text.push(ALPHABET[(byte % 32) as usize] as char);
    }
    Some(text)
}

/// The stored form of one code, bound to the person it was issued to.
///
/// Bound, because a recovery code is not a password: a hash two people could share would let one person's stored value confirm
/// the other's code. A plain digest is deliberate - the code already carries fifty bits of entropy, so there is nothing a slow
/// hash would buy, and a check costing a quarter of a second per stored code per sign-in is a check that gets made once and
/// then quietly deleted.
pub fn hash_for(person: &str, code: &str) -> String {
    let normalised = normalise(code).unwrap_or_default();
    digest(&format!("{person}:{normalised}"))
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Read a code a person typed: dashes, spaces, lower case and the letters Crockford folds are all accepted.
///
/// Letters are folded the way the alphabet intends - I and L as 1, O as 0, U as V - because a person reading `0` off a screen
/// writes `O`, and a person reading `1` writes `l`. Returns the ten characters, or `None` when what was typed cannot be a code
/// at all.
pub fn normalise(text: &str) -> Option<String> {
    let mut out = String::with_capacity(DIGITS);
    for character in text.chars() {
        if matches!(character, ' ' | '-' | '_' | '\n' | '\r' | '\t') {
            continue;
        }
        let upper = character.to_ascii_uppercase();
        let folded = match upper {
            'I' | 'L' => '1',
            'O' => '0',
            'U' => 'V',
            other => other,
        };
        if !ALPHABET.contains(&(folded as u8)) {
            return None;
        }
        out.push(folded);
        if out.len() > DIGITS {
            return None;
        }
    }
    (out.len() == DIGITS).then_some(out)
}

/// Find the stored code this one is, returning its position so the caller can spend it.
///
/// Constant time over the whole list and with no early exit, so the answer says nothing about how close a guess was or how many
/// codes are left in a shape a caller could time.
pub fn find(person: &str, code: &str, stored: &[String]) -> Option<usize> {
    let wanted = hash_for(person, code);
    let mut found: Option<usize> = None;
    for (index, one) in stored.iter().enumerate() {
        let equal: bool = one.as_bytes().ct_eq(wanted.as_bytes()).into();
        if equal && found.is_none() {
            found = Some(index);
        }
    }
    found
}

/// Whether a typed value looks like a recovery code rather than an app code, so a door can tell the two apart before trying.
///
/// A recovery code has letters in it; a six-digit app code has none. Judging by length alone is not enough, because a person
/// pastes a whole line out of their notes with a label in front of it.
pub fn looks_like_one(text: &str) -> bool {
    normalise(text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_is_ten_distinct_codes_written_in_groups() {
        let (shown, held) = issue("tariro");
        assert_eq!(shown.len(), COUNT);
        assert_eq!(held.len(), COUNT);
        for one in &shown {
            assert_eq!(one.len(), DIGITS + 1, "{one}");
            assert_eq!(one.chars().filter(|c| *c == '-').count(), 1, "{one}");
            assert!(normalise(one).is_some(), "{one}");
            // the letters that are read as digits are never issued, so a code on paper cannot be argued about
            assert!(
                !one.chars().any(|c| matches!(c, 'I' | 'L' | 'O' | 'U')),
                "{one}"
            );
        }
        let mut unique = shown.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), COUNT, "a repeated code is a wasted code");
    }

    #[test]
    fn what_is_stored_can_be_checked_and_nothing_else_can() {
        let (shown, held) = issue("tariro");
        let one = &shown[3];
        assert_eq!(find("tariro", one, &held), Some(3));
        // dashes, spacing and case are the person's business
        let typed = format!(" {} ", one.replace('-', " ").to_lowercase());
        assert_eq!(find("tariro", &typed, &held), Some(3));
        // and a near miss is nothing
        assert_eq!(find("tariro", &one.replace('-', "x"), &held), None);
    }

    #[test]
    fn a_code_stored_for_one_person_is_not_a_code_for_another() {
        let (shown, _) = issue("tariro");
        let one = &shown[0];
        let for_tariro = vec![hash_for("tariro", one)];
        assert_eq!(find("tariro", one, &for_tariro), Some(0));
        assert_eq!(find("samuel", one, &for_tariro), None);
    }

    #[test]
    fn a_person_reading_the_wrong_letter_is_still_let_in() {
        // 1 and l, 0 and O, V and U are the pairs a handwritten code mixes up
        assert_eq!(normalise("ABCDE-0l234").as_deref(), Some("ABCDE01234"));
        assert_eq!(normalise("ABCDE-O1234").as_deref(), Some("ABCDE01234"));
        assert_eq!(normalise("ABCDE-01234").as_deref(), Some("ABCDE01234"));
        // a character the alphabet has no reading for is not guessed at
        assert_eq!(normalise("ABCDE-01!34"), None);
    }

    #[test]
    fn something_that_is_not_a_code_is_refused() {
        assert_eq!(
            normalise("123456"),
            None,
            "an app code is not a recovery code"
        );
        assert!(looks_like_one("abcde-ghjkm"));
        assert!(!looks_like_one("123456"));
        assert!(normalise("").is_none());
        assert!(normalise("ABCDE-0123456").is_none());
    }
}
