#![forbid(unsafe_code)]
//! When two devices change the same record.
//!
//! An entry is an opaque, already-encrypted operation, so the server cannot read what it changes. That is the design: the
//! Book's key never reaches it. But "the server cannot see the change" is not the same as "the server cannot see a
//! collision", and the difference matters most where it is least visible - two people editing the same journal entry on
//! two devices, both offline, both syncing on the same morning.
//!
//! So an entry may *declare* what it was made against: a record key and the version of that record its author had in front
//! of them. The key is opaque to the server, a name the client chooses; the version is a number. An entry that declares
//! nothing is an append-only event and can never conflict, which is what most entries are and why nothing changes for a
//! client that does not use this.
//!
//! When an entry does declare a record, the rule is refusal by default:
//!
//! * the version it names is the version the Book holds - accept it, and the record moves on one;
//! * the Book has moved past that version and the record belongs to a family where losing an update costs nothing worth
//!   refusing a sync over, such as a note or a draft - accept it, last writer wins, and say so;
//! * otherwise - refuse, and say which record, which version the entry was made against and which version the Book is at,
//!   because a bookkeeper who is told "conflict" learns nothing while one who is told "journal_entry:42 was at 3, it is
//!   now at 4" knows exactly what to reopen.
//!
//! Accounting families are not on the list, and are not meant to be: a lost ledger update is a wrong balance, and a ledger
//! entry is immutable, so a correction is a new entry rather than an edit of an old one.
//!
//! The batch is decided before any of it is written. A refusal therefore stores nothing, which is what makes resuming
//! simple: the device sends what it has, is told which entry to look at, and nothing half-applied is left behind.

use serde_json::{json, Value};

use vimbai_store::read::{number, text};
use vimbai_store::shared::SharedRecords;
use vimbai_store::{Graph, RecordKind, StoreError};

/// The version a Book's record has reached, and the entry that took it there.
pub const RECORD_VERSION: RecordKind =
    RecordKind::new("SyncRecordVersion", "OWNS_SYNC_RECORD_VERSION");

/// The record families where losing an update costs nothing worth refusing a sync over. Anything not named here refuses,
/// so the list is a permission rather than a default: adding a family is a decision somebody makes on purpose.
pub const MAY_LOSE_UPDATES: [&str; 6] =
    ["note", "draft", "preference", "view", "setting", "template"];

/// The family a record key names: `journal_entry:42` is a `journal_entry`.
pub fn family_of(key: &str) -> &str {
    key.split(':').next().unwrap_or(key)
}

/// Whether a lost update to this record is safe to swallow.
pub fn may_lose_updates(key: &str) -> bool {
    MAY_LOSE_UPDATES.contains(&family_of(key))
}

/// A record, and the version of it an entry was made against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The client's own name for the record. The server never interprets it beyond its family.
    pub key: String,
    /// The version its author had in front of them. Zero for a record they believe is new.
    pub version: i64,
}

impl Claim {
    /// A claim, or why an entry's declaration is not one.
    pub fn parse(declared: &Value) -> Result<Claim, StoreError> {
        let key = text(declared, "key");
        if key.is_empty() {
            return Err(StoreError::Invalid(
                "a declared record needs a key".to_string(),
            ));
        }
        let version = number(declared, "version") as i64;
        if version < 0 {
            return Err(StoreError::Invalid(format!(
                "a declared version cannot be negative: {version}"
            )));
        }
        Ok(Claim { key, version })
    }
}

/// The version a Book's record has reached, or None when nothing has claimed it yet.
pub fn held(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
    key: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(SharedRecords::new(graph, RECORD_VERSION)
        .where_eq(book_id, "record_key", &json!(key))?
        .into_iter()
        .next())
}

/// The version number on a held record. A record with no number is version zero.
fn version_of(record: &Value) -> i64 {
    number(record, "version") as i64
}

/// One claim decided: the version it leaves the record at, and whether it won by last-write-wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub key: String,
    pub version: i64,
    pub last_write_wins: bool,
}

/// Two entries made against the same version of the same record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub key: String,
    pub claimed: i64,
    pub current: i64,
    pub held_by: String,
}

impl Conflict {
    /// What the author is told. It names the record and both versions, because "conflict" alone leaves somebody to guess
    /// which of their records to reopen.
    pub fn reason(&self) -> String {
        format!(
            "{} was changed by another device: this entry was made against version {}, the book is at {} ({})",
            self.key, self.claimed, self.current, self.held_by
        )
    }
}

/// What a batch would do, decided before anything is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// One per claim that may proceed, in the order the claims were made.
    pub accepted: Vec<Accepted>,
    /// The claims that stop the batch. Empty means the batch may be written.
    pub conflicts: Vec<Conflict>,
}

impl Plan {
    /// The refusal, if the batch cannot proceed. The first conflict is the one reported: fixing it and sending again is
    /// how a device makes progress, and a list of every conflict would be a list of guesses about a batch it is about to
    /// resend anyway.
    pub fn refusal(&self) -> Option<StoreError> {
        self.conflicts
            .first()
            .map(|conflict| StoreError::Conflict(conflict.reason()))
    }
}

/// Decide a batch of claims against what the Book holds, in order, without writing anything.
///
/// Claims are decided in sequence against a running picture, so one batch that changes the same record twice - a create
/// and then an edit of it - is accepted when its own versions are consecutive. That is a device catching up with itself,
/// not a conflict.
pub fn plan(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
    claims: &[Claim],
) -> Result<Plan, StoreError> {
    let mut picture: Vec<(String, i64, String)> = Vec::new();
    let mut decided = Plan::default();
    for claim in claims {
        let (current, held_by) = match picture.iter().find(|(key, _, _)| key == &claim.key) {
            Some((_, version, by)) => (*version, by.clone()),
            None => match held(graph, book_id, &claim.key)? {
                Some(record) => (version_of(&record), text(&record, "entry_id")),
                None => (0, String::new()),
            },
        };
        if claim.version == current {
            picture.push((claim.key.clone(), current + 1, String::new()));
            decided.accepted.push(Accepted {
                key: claim.key.clone(),
                version: current + 1,
                last_write_wins: false,
            });
            continue;
        }
        if may_lose_updates(&claim.key) {
            picture.push((claim.key.clone(), current + 1, String::new()));
            decided.accepted.push(Accepted {
                key: claim.key.clone(),
                version: current + 1,
                last_write_wins: true,
            });
            continue;
        }
        decided.conflicts.push(Conflict {
            key: claim.key.clone(),
            claimed: claim.version,
            current,
            held_by,
        });
    }
    Ok(decided)
}

/// Record that a record has moved on, naming the entry that moved it.
pub fn advance(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
    accepted: &Accepted,
    entry_id: &str,
    now: &Value,
) -> Result<(), StoreError> {
    let rows = SharedRecords::new(graph, RECORD_VERSION);
    let fields = json!({
        "record_key": accepted.key,
        "version": accepted.version,
        "entry_id": entry_id,
        "updated_at": now,
    });
    match rows
        .where_eq(book_id, "record_key", &json!(accepted.key))?
        .into_iter()
        .next()
    {
        Some(existing) => {
            rows.update(book_id, &text(&existing, "id"), &fields)?;
        }
        None => {
            let mut body = fields;
            body["id"] = json!(vimbai_common::ids::service_id());
            rows.create(book_id, &body)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    const BOOK: &str = "book-1";

    fn book() -> MemoryGraph {
        MemoryGraph::default()
    }

    #[test]
    fn a_family_that_may_lose_updates_is_the_only_thing_that_may_lose_them() {
        for key in ["note:1", "draft:9", "preference:theme", "template:3"] {
            assert!(may_lose_updates(key), "{key} costs nothing to lose");
        }
        for key in [
            "journal_entry:42",
            "ledger:1",
            "invoice:7",
            "payment:2",
            "account:100",
        ] {
            assert!(
                !may_lose_updates(key),
                "{key} must never be silently overwritten"
            );
        }
    }

    #[test]
    fn an_entry_made_against_the_current_version_may_proceed() {
        let graph = book();
        let decided = plan(
            &graph,
            BOOK,
            &[Claim {
                key: "note:1".into(),
                version: 0,
            }],
        )
        .unwrap();
        assert_eq!(decided.conflicts, Vec::new());
        assert_eq!(decided.accepted[0].version, 1);
        assert!(!decided.accepted[0].last_write_wins);
    }

    #[test]
    fn a_ledger_entry_made_against_a_version_that_has_moved_is_refused_by_name() {
        let graph = book();
        let first = Claim {
            key: "journal_entry:42".into(),
            version: 0,
        };
        let decided = plan(&graph, BOOK, std::slice::from_ref(&first)).unwrap();
        advance(&graph, BOOK, &decided.accepted[0], "entry-a", &json!(1)).unwrap();

        // a second device, still holding version 0, sends its own edit of the same entry
        let stale = plan(&graph, BOOK, &[first]).unwrap();
        assert_eq!(stale.accepted, Vec::new());
        let reason = stale.refusal().expect("a refusal").to_string();
        assert!(reason.contains("journal_entry:42"), "{reason}");
        assert!(reason.contains("version 0"), "{reason}");
        assert!(reason.contains("at 1"), "{reason}");
        assert!(
            reason.contains("entry-a"),
            "it says which entry holds it: {reason}"
        );
        assert!(
            matches!(stale.refusal(), Some(StoreError::Conflict(_))),
            "a conflicting write is a 409, not a 400"
        );
    }

    #[test]
    fn a_note_made_against_a_version_that_has_moved_is_accepted_as_last_write_wins() {
        let graph = book();
        let first = Claim {
            key: "note:1".into(),
            version: 0,
        };
        let decided = plan(&graph, BOOK, std::slice::from_ref(&first)).unwrap();
        advance(&graph, BOOK, &decided.accepted[0], "entry-a", &json!(1)).unwrap();

        let stale = plan(&graph, BOOK, &[first]).unwrap();
        assert_eq!(stale.conflicts, Vec::new());
        assert!(
            stale.accepted[0].last_write_wins,
            "and says so rather than pretending"
        );
        assert_eq!(stale.accepted[0].version, 2, "the version still moves on");
    }

    #[test]
    fn one_batch_that_changes_a_record_twice_is_catching_up_with_itself() {
        let graph = book();
        let decided = plan(
            &graph,
            BOOK,
            &[
                Claim {
                    key: "journal_entry:9".into(),
                    version: 0,
                },
                Claim {
                    key: "journal_entry:9".into(),
                    version: 1,
                },
            ],
        )
        .unwrap();
        assert_eq!(decided.conflicts, Vec::new());
        assert_eq!(decided.accepted.len(), 2);
        assert_eq!(decided.accepted[1].version, 2);
    }

    #[test]
    fn a_claim_with_no_key_is_not_a_claim() {
        assert!(Claim::parse(&json!({"version": 1})).is_err());
        assert!(Claim::parse(&json!({"key": "note:1", "version": -1})).is_err());
        assert_eq!(
            Claim::parse(&json!({"key": "note:1", "version": 3})).unwrap(),
            Claim {
                key: "note:1".into(),
                version: 3
            }
        );
    }
}
