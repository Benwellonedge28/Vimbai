#![forbid(unsafe_code)]
//! Schema changes, as ordered steps both stores apply the same way.
//!
//! A store that has been running is not a store that can be recreated: it holds a person's ledger. So the schema is not a
//! `CREATE TABLE` run on every open - it is a list of numbered steps, a recorded version, and one rule about what to do
//! with a version the code does not know.
//!
//! The rule lives here rather than in each backend because two backends that disagree about it is how one of them ends up
//! silently writing to a schema it does not understand. The SQL is the backend's own business: Postgres says `jsonb` and
//! SQLite says `TEXT`, and neither can be the other's statement.

use crate::StoreError;

/// One forward step of a schema, with the step that undoes it.
///
/// A migration that cannot be rolled back is a migration nobody dares to run against a store holding real records, so the
/// two halves are written together. A test applies, undoes and re-applies them, which is the only way to know the second
/// step is not a fiction.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub name: &'static str,
    pub up: &'static str,
    pub down: &'static str,
}

/// What opening a store has to do about its schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// The store is at the version this build knows; nothing to run.
    UpToDate { version: usize },
    /// Run steps `from..` in order, recording each one.
    Apply { from: usize, to: usize },
    /// The store was written by a newer build, which knows steps this one does not.
    ///
    /// Refused rather than guessed at: running the steps this build knows would record a version that is lower than the
    /// schema actually in the database, and the next step would then be written against a shape it never expected.
    WrittenByNewer { found: i64, known: usize },
}

/// What to do, given the version the store reports and the steps this build has.
///
/// A store reporting `0` has no schema yet. A store reporting more steps than this build has was written by a newer build.
pub fn plan(found: i64, steps: &[Step]) -> Plan {
    if found < 0 {
        return Plan::Apply {
            from: 0,
            to: steps.len(),
        };
    }
    let found = found as usize;
    if found > steps.len() {
        Plan::WrittenByNewer {
            found: found as i64,
            known: steps.len(),
        }
    } else if found == steps.len() {
        Plan::UpToDate { version: found }
    } else {
        Plan::Apply {
            from: found,
            to: steps.len(),
        }
    }
}

/// The refusal a store written by a newer build gets, worded once so both backends say the same thing.
pub fn newer_than_this_build(found: i64, known: usize) -> StoreError {
    StoreError::Graph(format!(
        "this store was written by a newer build: schema {found}, this build knows {known}. Upgrade the engine rather \
         than opening it with an older one, which would write the next step against a schema it cannot see"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEPS: &[Step] = &[
        Step {
            name: "0001",
            up: "",
            down: "",
        },
        Step {
            name: "0002",
            up: "",
            down: "",
        },
    ];

    #[test]
    fn an_empty_store_runs_every_step() {
        assert_eq!(plan(0, STEPS), Plan::Apply { from: 0, to: 2 });
    }

    #[test]
    fn a_store_behind_by_one_runs_only_the_step_it_is_missing() {
        assert_eq!(plan(1, STEPS), Plan::Apply { from: 1, to: 2 });
    }

    #[test]
    fn a_store_at_the_current_version_runs_nothing() {
        assert_eq!(plan(2, STEPS), Plan::UpToDate { version: 2 });
    }

    #[test]
    fn a_store_written_by_a_newer_build_is_refused_and_not_downgraded() {
        assert_eq!(plan(3, STEPS), Plan::WrittenByNewer { found: 3, known: 2 });
        let message = format!("{}", newer_than_this_build(3, 2));
        assert!(
            message.contains("newer build") && message.contains("Upgrade the engine"),
            "the refusal has to say what to do: {message}"
        );
    }

    #[test]
    fn a_store_reporting_nonsense_is_treated_as_new_and_migrated() {
        // A backend that cannot read a version row at all answers -1 rather than failing: the schema steps are written to
        // be safe to re-run, so creating what is missing is better than refusing to open a store.
        assert_eq!(plan(-1, STEPS), Plan::Apply { from: 0, to: 2 });
    }
}
