#![forbid(unsafe_code)]
//! What a password has to be, and what it is before it is hashed.
//!
//! The rules here follow NIST SP 800-63B and OWASP's password guidance, which is what "standard and modern" means in
//! practice, and they replace the shipped check, which was a length limit and nothing else:
//!
//! * **Length is the requirement, composition is not.** At least 8 characters, counted as characters rather than bytes so
//!   a password written in Amharic or Devanagari is not rejected for being shorter than it looks. No "one capital, one
//!   digit, one symbol" rule: those rules are known to push people towards `Password1!`, and NIST removed them for that
//!   reason. Nothing expires: forcing a rotation is how `Summer2024!` becomes `Autumn2024!`.
//! * **Obviously guessable passwords are refused** - the ones on every published list, a single repeated character, the
//!   person's own username or the part of their email before the `@`, and the service's own name.
//! * **Unicode is normalised** (NFKC) before hashing, so the same password typed on a phone keyboard and on a laptop is the
//!   same password. Hashing normalises; checking tries the stored form first, then the normalised one, so an account made
//!   before this existed still signs in.
//! * **A good hash, chosen once, here.** bcrypt at cost 12 today; the stored prefix says which algorithm made it, so a
//!   later move to Argon2id is `needs_rehash` and a rehash on next sign-in, not a migration of every account.
//!
//! It is a plain library: who may set a password (an account owner, an administrator, a reset link) is the caller's
//! question, and the answer is the same set of rules either way.

use unicode_normalization::UnicodeNormalization;

/// The shortest password accepted, in characters.
pub const MIN_CHARACTERS: usize = 8;

/// The longest password worth sending to bcrypt, in bytes. bcrypt reads at most this many and silently ignores the rest,
/// so a longer password is refused rather than quietly cut down to something the person did not choose.
pub const MAX_BYTES: usize = 72;

/// bcrypt's cost. 12 is OWASP's floor for bcrypt and about 250ms on the hardware this ships to.
pub const COST: u32 = 12;

/// Why a password was refused. The message is what a person reads, so it says what to do and not what was wrong with them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordFault {
    /// Shorter than [`MIN_CHARACTERS`] characters.
    TooShort,
    /// Longer than bcrypt can hash.
    TooLong,
    /// On the list everybody tries.
    TooCommon,
    /// One character, repeated.
    OnlyOneCharacter,
    /// Contains the name it is protecting.
    NamesTheAccount,
}

impl PasswordFault {
    /// What to tell the person. No rule is described as a complexity requirement, because there is none.
    pub fn message(self) -> &'static str {
        match self {
            PasswordFault::TooShort => "Use at least 8 characters. A short phrase is easier to remember than a short word.",
            PasswordFault::TooLong => "Use at most 72 bytes of password. That is roughly 72 characters in English.",
            PasswordFault::TooCommon => "That password is one of the most commonly used. Choose something nobody else would.",
            PasswordFault::OnlyOneCharacter => "Use more than one distinct character.",
            PasswordFault::NamesTheAccount => "Do not use your own username or email address as your password.",
        }
    }
}

/// The passwords that appear at the top of every published list. Not a substitute for a breach corpus - that is a network
/// call and belongs to a deployment that can make one - but it stops the worst of them with no network at all.
const COMMON: [&str; 32] = [
    "password",
    "password1",
    "password123",
    "passw0rd",
    "12345678",
    "123456789",
    "1234567890",
    "qwertyui",
    "qwerty123",
    "qwertyuiop",
    "iloveyou",
    "letmein",
    "welcome",
    "welcome1",
    "admin123",
    "administrator",
    "abc12345",
    "monkey123",
    "dragon123",
    "sunshine",
    "princess",
    "football",
    "baseball",
    "trustno1",
    "starwars",
    "whatever",
    "changeme",
    "secret123",
    "vimbai123",
    "vimbai2024",
    "vimbai2025",
    "vimbai2026",
];

/// The form a password is hashed in: NFKC, so a composed and a decomposed accent are one password.
pub fn normalise(password: &str) -> String {
    password.nfkc().collect()
}

/// Whether `password` may become somebody's password.
///
/// `account_names` are the strings that identify the person - their username, their email address - and a password
/// containing one of them (case-insensitively) is refused: those are the first guesses anyone makes.
pub fn check(password: &str, account_names: &[&str]) -> Result<(), PasswordFault> {
    let normalised = normalise(password);
    if normalised.chars().count() < MIN_CHARACTERS {
        return Err(PasswordFault::TooShort);
    }
    if normalised.len() > MAX_BYTES {
        return Err(PasswordFault::TooLong);
    }
    let folded = normalised.to_lowercase();
    if folded.chars().count() > 1
        && folded
            .chars()
            .all(|one| one == folded.chars().next().unwrap())
    {
        return Err(PasswordFault::OnlyOneCharacter);
    }
    if COMMON.contains(&folded.as_str()) {
        return Err(PasswordFault::TooCommon);
    }
    for name in account_names {
        for piece in name_pieces(name) {
            if piece.chars().count() >= 3 && folded.contains(&piece) {
                return Err(PasswordFault::NamesTheAccount);
            }
        }
    }
    Ok(())
}

/// The parts of a name worth refusing inside a password: the whole thing, and the part before the `@` of an email.
fn name_pieces(name: &str) -> Vec<String> {
    let folded = name.trim().to_lowercase();
    let mut pieces = vec![folded.clone()];
    if let Some((local, _)) = folded.split_once('@') {
        if !local.is_empty() {
            pieces.push(local.to_string());
        }
    }
    pieces
}

/// Hash a password for storage, or say why it cannot be one.
pub fn hash(password: &str, account_names: &[&str]) -> Result<String, PasswordFault> {
    check(password, account_names)?;
    bcrypt::hash(normalise(password), COST).map_err(|_| PasswordFault::TooLong)
}

/// Check a password against a stored hash.
///
/// Two attempts, in this order: the password as typed, then the normalised form. The first covers every account made
/// before normalisation existed; the second covers the same password typed in a different Unicode form. A hash this build
/// cannot read is a failed password, never a different answer, so a broken record cannot be told apart from a wrong guess.
pub fn verify(password: &str, stored: &str) -> bool {
    if password.is_empty() {
        return false;
    }
    let normalised = normalise(password);
    if normalised.len() <= MAX_BYTES && bcrypt::verify(&normalised, stored).unwrap_or(false) {
        return true;
    }
    if password != normalised && password.len() <= MAX_BYTES {
        return bcrypt::verify(password, stored).unwrap_or(false);
    }
    false
}

/// Whether a stored hash was made with today's cost or normalisation, and is worth replacing on the next sign-in.
///
/// bcrypt's own prefix carries the cost (`$2b$12$...`), so this needs no side table: the record says how it was made.
pub fn needs_rehash(stored: &str) -> bool {
    let Some(rest) = stored.strip_prefix("$2") else {
        return true;
    };
    let mut parts = rest.splitn(3, '$');
    let _variant = parts.next();
    let Some(cost) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
        return true;
    };
    cost < COST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_is_the_requirement_and_eight_characters_is_the_floor() {
        assert_eq!(check("short", &[]), Err(PasswordFault::TooShort));
        assert_eq!(check("seven77", &[]), Err(PasswordFault::TooShort));
        assert_eq!(check("eight888", &[]), Ok(()));
    }

    #[test]
    fn a_long_passphrase_is_the_point_rather_than_a_symbol() {
        // no capitals, no digits, no symbols: refused by the old rules, accepted here, and stronger than `Passw0rd!`
        assert_eq!(check("correct horse battery staple", &[]), Ok(()));
        assert_eq!(check("i like tea", &[]), Ok(()));
    }

    #[test]
    fn characters_are_counted_as_characters_not_bytes() {
        // eight characters, twenty-four bytes: a byte count would call this long enough and a naive limit would refuse it
        let amharic = "መለያቃልፍቃል";
        assert!(amharic.chars().count() >= MIN_CHARACTERS);
        assert!(
            amharic.len() > MIN_CHARACTERS,
            "and it is longer still in bytes, so a byte limit would not have refused it either"
        );
        assert_eq!(check(amharic, &[]), Ok(()));
        // seven characters in a script that takes three bytes each is still short
        assert_eq!(check("መለያቃልፍቃ", &[]), Err(PasswordFault::TooShort));
    }

    #[test]
    fn the_passwords_everyone_tries_are_refused_whatever_their_case() {
        // each of these is long enough, so it is refused for being common rather than for being short
        for guess in [
            "password",
            "PASSWORD",
            "Password123",
            "iloveyou",
            "qwertyuiop",
            "changeme",
        ] {
            assert_eq!(check(guess, &[]), Err(PasswordFault::TooCommon), "{guess}");
        }
    }

    #[test]
    fn a_name_is_not_a_password_for_that_name() {
        assert_eq!(
            check("samuelmukandara", &["samuel", "sam@example.com"]),
            Err(PasswordFault::NamesTheAccount)
        );
        assert_eq!(
            check("my email is sam", &["samuel", "sam@example.com"]),
            Err(PasswordFault::NamesTheAccount),
            "the part of an email before the @ is a name too"
        );
        // a two-character name is too short to be worth refusing: 'jo' appears inside ordinary words
        assert_eq!(check("majority rules", &["jo"]), Ok(()));
    }

    #[test]
    fn a_single_repeated_character_is_not_a_password() {
        assert_eq!(check("aaaaaaaa", &[]), Err(PasswordFault::OnlyOneCharacter));
        assert_eq!(check("........", &[]), Err(PasswordFault::OnlyOneCharacter));
    }

    #[test]
    fn the_same_password_typed_in_two_unicode_forms_is_one_password() {
        let composed = "caf\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}"; // é written as one character
        let decomposed = "cafe\u{301}e\u{301}e\u{301}e\u{301}e\u{301}"; // e + combining acute
        assert_ne!(composed, decomposed);
        let stored = hash(composed, &[]).expect("a hashable password");
        assert!(verify(composed, &stored));
        assert!(
            verify(decomposed, &stored),
            "the same password on another keyboard"
        );
    }

    #[test]
    fn a_hash_made_before_normalisation_still_signs_in() {
        let typed = "my long passphrase";
        let legacy = bcrypt::hash(typed, 4).expect("a hash");
        assert!(verify(typed, &legacy));
        assert!(!verify("my long passphrasf", &legacy));
    }

    #[test]
    fn a_hash_this_build_cannot_read_is_a_failed_password_and_not_an_error() {
        assert!(!verify("anything at all", "not-a-hash"));
        assert!(!verify("anything at all", ""));
        assert!(!verify("", "not-a-hash"));
    }

    #[test]
    fn today_s_cost_is_recognised_and_an_older_one_is_worth_replacing() {
        let fresh = bcrypt::hash("whatever", COST).expect("a hash");
        assert!(!needs_rehash(&fresh));
        let old = bcrypt::hash("whatever", 8).expect("a hash");
        assert!(needs_rehash(&old));
        assert!(needs_rehash("not-a-hash"));
    }

    #[test]
    fn an_over_long_password_is_refused_rather_than_quietly_cut_down() {
        let long = "a".repeat(MAX_BYTES + 1);
        assert_eq!(check(&long, &[]), Err(PasswordFault::TooLong));
    }
}
