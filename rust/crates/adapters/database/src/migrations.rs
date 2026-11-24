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

/// Whether this process is allowed to change a store's schema, or only to check it.
///
/// A deployment whose database credentials cannot create tables - which is most managed databases, and the right way to
/// run one - must not have the engine try. `verify` says so: the schema is expected to be brought up by whatever deploys
/// the engine, and a store found behind is refused with the version it is at, rather than half-migrated by an application
/// that was never given the right to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaMode {
    Apply,
    Verify,
}

/// What this process was told to do about schemas: `VIMBAI_SCHEMA_MODE=apply|verify`, applying by default.
///
/// The default is apply because that is what a laptop, a device and a self-hosted deployment want, and because a store
/// that silently stays behind is worse than one that upgrades itself. A managed deployment sets `verify` once and the
/// engine stops trying to be its database administrator.
pub fn mode_from_env() -> SchemaMode {
    match std::env::var("VIMBAI_SCHEMA_MODE") {
        Ok(value) if value.trim().eq_ignore_ascii_case("verify") => SchemaMode::Verify,
        _ => SchemaMode::Apply,
    }
}

/// What to do about a store's schema, given what it reports and what this process is allowed to do.
///
/// `None` means there is nothing to run. `Some((from, to))` means run those steps. A store written by a newer build is
/// refused either way; a store behind, in verify mode, is refused with the version it is at and the one expected, so an
/// operator knows what to run without opening the database by hand.
pub fn decide(
    found: i64,
    steps: &[Step],
    mode: SchemaMode,
    store: &str,
) -> Result<Option<(usize, usize)>, StoreError> {
    match plan(found, steps) {
        Plan::UpToDate { .. } => Ok(None),
        Plan::Apply { from, to } => match mode {
            SchemaMode::Apply => Ok(Some((from, to))),
            SchemaMode::Verify => Err(StoreError::Graph(format!(
                "the {store} schema is at {found} and this build expects {}; this process is set to verify only \
                 (VIMBAI_SCHEMA_MODE=verify), so it will not change it. Bring the schema up where the engine is \
                 deployed, or run once with VIMBAI_SCHEMA_MODE=apply.",
                steps.len()
            ))),
        },
        Plan::WrittenByNewer { found, known } => Err(newer_than_this_build(found, known)),
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

    #[test]
    fn a_store_behind_is_refused_when_this_process_may_only_verify() {
        let refused = decide(0, STEPS, SchemaMode::Verify, "deployment")
            .expect_err("verify mode refuses a store that is behind");
        let said = refused.to_string();
        assert!(said.contains("deployment schema is at 0"), "{said}");
        assert!(said.contains("VIMBAI_SCHEMA_MODE=verify"), "{said}");
        assert!(
            said.contains("run once with VIMBAI_SCHEMA_MODE=apply"),
            "the way out is named: {said}"
        );
    }

    #[test]
    fn a_store_behind_is_brought_up_when_this_process_may_apply() {
        assert_eq!(
            decide(0, STEPS, SchemaMode::Apply, "device").expect("apply"),
            Some((0, STEPS.len()))
        );
        assert_eq!(
            decide(1, STEPS, SchemaMode::Apply, "device").expect("apply"),
            Some((1, STEPS.len()))
        );
    }

    #[test]
    fn a_store_at_this_build_has_nothing_to_do_in_either_mode() {
        let steps = STEPS.len() as i64;
        assert_eq!(decide(steps, STEPS, SchemaMode::Apply, "device"), Ok(None));
        assert_eq!(decide(steps, STEPS, SchemaMode::Verify, "device"), Ok(None));
    }

    #[test]
    fn a_store_written_by_a_newer_build_is_refused_in_both_modes() {
        let ahead = STEPS.len() as i64 + 1;
        for mode in [SchemaMode::Apply, SchemaMode::Verify] {
            let said = decide(ahead, STEPS, mode, "device")
                .expect_err("a newer store is refused")
                .to_string();
            assert!(said.contains("newer build"), "{said}");
        }
    }
}
