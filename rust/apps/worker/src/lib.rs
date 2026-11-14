#![forbid(unsafe_code)]
//! The scheduled-automation worker.
//!
//! `domains/automation` could already record a rule, walk its steps and answer its history, but nothing ran a rule the
//! clock was due to start. This does that, and the hard part is not running a rule: it is running it exactly once.
//!
//! A worker can be killed between claiming a run and recording that it finished - a redeploy, a full disk, a container
//! that is simply stopped. If a restart then started the run again, a rule that posts a journal entry would post it
//! twice, and the second posting is indistinguishable from the first. So a run's identity is not a fresh id: it is the
//! rule and the instant it was due, `run-{rule_id}-{due_at}`. Claiming means writing a run row under that id. A second
//! worker cannot write it, so it cannot start the run; a worker that restarts finds the row it already wrote, and
//! finishes *that* run rather than beginning another. Completing advances the rule's due instant past now, so the same
//! due cannot fire again even after the row is gone.
//!
//! Nothing here reads the clock. Every entry point takes `now` as a parameter, because a scheduler that consults the
//! clock cannot be tested: the test that proves "a rule due later does not fire" would have to wait until later.
//!
//! ## The schedule a rule carries
//!
//! A rule's `condition` is a free-form object, and the schedule lives in it under `schedule`:
//!
//! ```json
//! { "schedule": { "due_at": 1763212800, "repeat_seconds": 3600 } }
//! ```
//!
//! `due_at` is the next instant the rule is due, in seconds since the epoch - the same reading
//! `vimbai_common::clock::moment()` returns and the same unit the services already stamp, so a rule compares against the
//! store without a date parser in the way. `repeat_seconds` is optional: without it the rule fires once and is left
//! alone; with it the worker advances the instant by whole periods until it is in the future.

use automation::automations::{walk_steps, EXECUTION, RULE};
use serde_json::{json, Value};
use store::read::{flag, text};
use store::{BookScope, Graph, Records, StoreError};

/// What claiming a run came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// This worker wrote the run row, so it is the one that must finish it.
    Started,
    /// Another attempt claimed this run already and did not finish it. A crash between claim and completion leaves
    /// exactly this, and the run is claimable once: whoever finds it finishes it, and nobody starts a second one.
    Resumed,
    /// The run is already recorded, so it is not run again.
    AlreadyDone,
}

/// The store row a run is recorded under: the rule and the instant it was due, and nothing else.
///
/// This is the whole idempotency argument, in one function. Two attempts on the same due instant produce the same id,
/// so one of them loses the write and the other cannot start a second run.
pub fn run_id(rule_id: &str, due_at: f64) -> String {
    format!("run-{rule_id}-{}", due_key(due_at))
}

/// A due instant as an id can carry it: epoch seconds, exactly, and never in exponent form.
fn due_key(due_at: f64) -> String {
    let whole = due_at.trunc();
    if (due_at - whole).abs() < f64::EPSILON {
        format!("{whole:.0}")
    } else {
        format!("{due_at:.6}")
    }
}

/// The instant this rule is next due, when the rule says.
pub fn due_at_of(rule: &Value) -> Option<f64> {
    rule.get("condition")?
        .get("schedule")?
        .get("due_at")?
        .as_f64()
}

/// How often the rule repeats, when it does.
pub fn repeat_of(rule: &Value) -> Option<f64> {
    rule.get("condition")?
        .get("schedule")?
        .get("repeat_seconds")?
        .as_f64()
        .filter(|seconds| *seconds > 0.0)
}

/// Whether this rule is one the clock starts: enabled, and triggered by a schedule.
pub fn is_scheduled(rule: &Value) -> bool {
    flag(rule, "enabled") && text(rule, "trigger") == "scheduled"
}

/// Take this run, or learn who already has it.
pub fn claim(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule: &Value,
    due_at: f64,
    now: f64,
) -> Result<(Claim, String), StoreError> {
    let rule_id = text(rule, "id");
    let id = run_id(&rule_id, due_at);
    let runs = Records::new(graph, EXECUTION);
    match runs.get(scope, &id) {
        Ok(held) => {
            if text(&held, "status") == "completed" {
                return Ok((Claim::AlreadyDone, id));
            }
            // the row exists and was never finished: this attempt carries it over rather than opening a second one
            Ok((Claim::Resumed, id))
        }
        Err(StoreError::NotFound) => {
            runs.create(
                scope,
                &json!({
                    "id": id,
                    "rule_id": rule_id,
                    "company_id": text(rule, "company_id"),
                    "status": "running",
                    "scheduled_for": due_at,
                    "started_at": now,
                    "step_results": [],
                    "error": Value::Null,
                }),
            )?;
            Ok((Claim::Started, id))
        }
        Err(other) => Err(other),
    }
}

/// Run one rule's due work, and answer what happened to it.
fn run_one(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule: &Value,
    due_at: f64,
    now: f64,
) -> Result<Value, StoreError> {
    let (claim, id) = claim(graph, scope, rule, due_at, now)?;
    let rule_id = text(rule, "id");
    if claim == Claim::AlreadyDone {
        return Ok(json!({
            "rule_id": rule_id,
            "execution_id": id,
            "outcome": "already recorded",
        }));
    }

    let steps = rule
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let outcome = walk_steps(&steps);
    let failed = outcome.error.is_some();
    let status = if failed { "failed" } else { "completed" };

    Records::new(graph, EXECUTION).update(
        scope,
        &id,
        &json!({
            "status": status,
            "completed_at": now,
            "step_results": outcome.steps,
            "error": outcome.error,
        }),
    )?;

    // a run a person asked for is a run a person should hear about: it lands in the owner's inbox
    let name = {
        let held = text(rule, "name");
        if held.is_empty() {
            rule_id.clone()
        } else {
            held
        }
    };
    notifications::notifications::create_notification(
        graph,
        scope,
        &json!({
            "type": if failed { "workflow_failed" } else { "workflow_completed" },
            "title": if failed { "Automation failed" } else { "Automation completed" },
            "message": if failed {
                format!("{name} stopped: {}", outcome.error.clone().unwrap_or_default())
            } else {
                format!("{name} ran and completed")
            },
            "priority": "normal",
            "channels": ["in_app"],
            "recipients": [scope.user_id.clone()],
            "metadata": { "rule_id": rule_id, "execution_id": id, "scheduled_for": due_at },
            "created_at": now,
        }),
    )?;

    advance(graph, scope, rule, due_at, now)?;

    Ok(json!({
        "rule_id": rule_id,
        "execution_id": id,
        "outcome": match claim {
            Claim::Started => "ran",
            Claim::Resumed => "resumed after an unfinished attempt",
            Claim::AlreadyDone => "already recorded",
        },
        "status": status,
        "steps": steps.len(),
    }))
}

/// Move the rule's next due instant past `now`, so one due cannot fire twice.
fn advance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule: &Value,
    due_at: f64,
    now: f64,
) -> Result<(), StoreError> {
    let Some(repeat) = repeat_of(rule) else {
        return Ok(());
    };
    let periods = ((now - due_at) / repeat).floor().max(0.0) + 1.0;
    let next = due_at + periods * repeat;
    let mut condition = rule.get("condition").cloned().unwrap_or_else(|| json!({}));
    if let Some(schedule) = condition.get_mut("schedule") {
        schedule["due_at"] = json!(next);
    }
    Records::new(graph, RULE).update(
        scope,
        &text(rule, "id"),
        &json!({ "condition": condition }),
    )?;
    Ok(())
}

/// One tick over one Book: every scheduled rule that is due, run once.
pub fn tick_book(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    now: f64,
) -> Result<Vec<Value>, StoreError> {
    let mut answered = Vec::new();
    for rule in Records::new(graph, RULE).list(scope, 500)? {
        if !is_scheduled(&rule) {
            continue;
        }
        let Some(due_at) = due_at_of(&rule) else {
            continue;
        };
        if due_at > now {
            continue;
        }
        answered.push(run_one(graph, scope, &rule, due_at, now)?);
    }
    Ok(answered)
}

/// How long a request claim keeps its promise, in seconds. The api-server defines the same lifetime for the claim it
/// writes; a claim is only worth reading for as long as a client could still be retrying.
pub const CLAIM_LIFETIME_SECONDS: f64 = 86_400.0;

/// Remove the request claims whose promise has timed out. Answers how many went.
pub fn prune_claims(graph: &(impl Graph + ?Sized), now: f64) -> Result<usize, StoreError> {
    let claims = store::PlatformRecords::new(graph, store::IDEMPOTENCY_CLAIM);
    let mut removed = 0;
    for row in claims.matching("state", "done", 500)? {
        let created = row.get("created_at").and_then(Value::as_f64).unwrap_or(now);
        if now - created <= CLAIM_LIFETIME_SECONDS {
            continue;
        }
        if let Some(id) = row.get("id").and_then(Value::as_str) {
            if claims.remove(id)? {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// One tick over every Book in the store, which is what the worker process does on each pass.
///
/// Books are platform records with no owner of their own, so they are listed as such and each is then worked inside the
/// scope of the person who opened it - the same scope the engine serves that Book's requests under.
pub fn tick(graph: &(impl Graph + ?Sized), now: f64) -> Result<Vec<Value>, StoreError> {
    // claims first: a request claim is bookkeeping about a request, and one whose promise has timed out is litter the
    // store does not need. Pruning is here because this is the only process that wakes up on a clock.
    prune_claims(graph, now)?;
    let mut answered = Vec::new();
    let books = store::PlatformRecords::new(graph, offline_sync::sync::BOOK).list(1000)?;
    for book in books {
        let owner = text(&book, "created_by");
        let book_id = text(&book, "id");
        if owner.is_empty() || book_id.is_empty() {
            continue;
        }
        let scope = BookScope::new(owner, book_id);
        for run in tick_book(graph, &scope, now)? {
            answered.push(run);
        }
    }
    Ok(answered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use store::fake::MemoryGraph;

    #[test]
    fn a_claim_past_its_promise_is_removed_and_a_fresh_one_is_kept() {
        let graph = MemoryGraph::default();
        let claims = store::PlatformRecords::new(&graph, store::IDEMPOTENCY_CLAIM);
        claims
            .create(&json!({ "id": "stale", "state": "done", "created_at": 0.0 }))
            .expect("a stale claim");
        claims
            .create(&json!({ "id": "fresh", "state": "done", "created_at": 90000.0 }))
            .expect("a fresh claim");
        let removed = prune_claims(&graph, 100_000.0).expect("a prune");
        assert_eq!(removed, 1);
        assert!(claims.get("stale").expect("a read").is_none());
        assert!(claims.get("fresh").expect("a read").is_some());
    }

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn rule_with(graph: &MemoryGraph, due_at: f64, repeat: Option<f64>) -> Value {
        let schedule = match repeat {
            Some(seconds) => json!({ "due_at": due_at, "repeat_seconds": seconds }),
            None => json!({ "due_at": due_at }),
        };
        automation::automations::create_automation_rule(
            graph,
            &talked(),
            &json!({
                "id": "rule-1",
                "name": "Month end sweep",
                "company_id": "co-1",
                "trigger": "scheduled",
                "condition": { "schedule": schedule },
                "steps": [
                    { "step_id": "collect", "step_name": "Collect", "action": "post_entries" },
                    { "step_id": "close", "step_name": "Close", "action": "close_period",
                      "depends_on": ["collect"] }
                ],
            }),
        )
        .expect("a rule")
    }

    fn run_ids(answered: &[Value]) -> Vec<String> {
        answered
            .iter()
            .map(|run| text(run, "execution_id"))
            .collect()
    }

    #[test]
    fn a_run_is_named_by_its_rule_and_the_instant_it_was_due() {
        assert_eq!(run_id("rule-1", 1_763_212_800.0), "run-rule-1-1763212800");
        // a fractional instant keeps its fraction rather than colliding with the whole second beside it
        assert_ne!(run_id("rule-1", 1.5), run_id("rule-1", 1.0));
        assert_eq!(run_id("rule-1", 1.5), run_id("rule-1", 1.5));
    }

    #[test]
    fn a_rule_due_now_fires_once_and_not_again() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        rule_with(&graph, due, None);

        let first = tick_book(&graph, &talked(), due).expect("a tick");
        assert_eq!(first.len(), 1, "{first:?}");
        assert_eq!(first[0]["outcome"], json!("ran"));
        assert_eq!(first[0]["status"], json!("completed"));

        // the same due instant, asked again: the run is recorded, so nothing is started a second time
        let again = tick_book(&graph, &talked(), due + 60.0).expect("a tick");
        assert_eq!(again.len(), 1, "the rule is still due until it repeats");
        assert_eq!(again[0]["outcome"], json!("already recorded"));

        let runs = Records::new(&graph, EXECUTION)
            .list(&talked(), 10)
            .expect("the runs");
        assert_eq!(runs.len(), 1, "one run, not two: {runs:?}");
        assert_eq!(runs[0]["status"], json!("completed"));
        assert_eq!(
            runs[0]["step_results"].as_array().map(Vec::len),
            Some(2),
            "both steps were walked"
        );
    }

    #[test]
    fn a_rule_due_later_does_not_fire() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        rule_with(&graph, due, None);

        let early = tick_book(&graph, &talked(), due - 1.0).expect("a tick");
        assert!(early.is_empty(), "{early:?}");
        let runs = Records::new(&graph, EXECUTION)
            .list(&talked(), 10)
            .expect("the runs");
        assert!(runs.is_empty(), "nothing is claimed before it is due");
    }

    #[test]
    fn a_crash_between_claim_and_completion_leaves_the_run_claimable_exactly_once() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        let rule = rule_with(&graph, due, None);

        // the first attempt claims the run and then dies: the row says running, and nothing else happened
        let (first, id) = claim(&graph, &talked(), &rule, due, due).expect("a claim");
        assert_eq!(first, Claim::Started);
        let runs = Records::new(&graph, EXECUTION);
        assert_eq!(runs.list(&talked(), 10).expect("the runs").len(), 1);
        assert_eq!(
            runs.get(&talked(), &id).expect("the row")["status"],
            json!("running")
        );

        // a second worker on the same due instant cannot claim it either: the id is taken
        let (second, _) = claim(&graph, &talked(), &rule, due, due + 5.0).expect("a claim");
        assert_eq!(second, Claim::Resumed);

        // the next tick carries that run over and finishes it
        let recovered = tick_book(&graph, &talked(), due + 30.0).expect("a tick");
        assert_eq!(recovered.len(), 1, "{recovered:?}");
        assert_eq!(recovered[0]["execution_id"], json!(id));
        assert_eq!(
            recovered[0]["outcome"],
            json!("resumed after an unfinished attempt")
        );

        let after = runs.list(&talked(), 10).expect("the runs");
        assert_eq!(
            after.len(),
            1,
            "one run was finished, not a second one added"
        );
        assert_eq!(after[0]["status"], json!("completed"));

        // and it is done: nothing further is started
        let settled = tick_book(&graph, &talked(), due + 60.0).expect("a tick");
        assert_eq!(settled[0]["outcome"], json!("already recorded"));
    }

    #[test]
    fn a_completed_run_is_not_repeated_even_when_the_claim_row_is_rewritten_as_running() {
        // the one shape that could double-post: a row that says running after the work was done. The work is not
        // re-done on the strength of the row alone - it is re-done at all, which is why the id carries the due instant.
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        let rule = rule_with(&graph, due, None);
        let (_, id) = claim(&graph, &talked(), &rule, due, due).expect("a claim");
        Records::new(&graph, EXECUTION)
            .update(&talked(), &id, &json!({ "status": "completed" }))
            .expect("a finish");

        let (taken, _) = claim(&graph, &talked(), &rule, due, due + 1.0).expect("a claim");
        assert_eq!(taken, Claim::AlreadyDone);
        let answered = tick_book(&graph, &talked(), due + 1.0).expect("a tick");
        assert_eq!(answered[0]["outcome"], json!("already recorded"));
    }

    #[test]
    fn a_repeating_rule_moves_its_due_instant_past_now() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        rule_with(&graph, due, Some(3600.0));

        tick_book(&graph, &talked(), due + 10.0).expect("a tick");
        let rule = Records::new(&graph, RULE)
            .get(&talked(), "rule-1")
            .expect("the rule");
        // ten seconds after the due instant, the next one is an hour on; three hours later it is the fourth
        assert_eq!(due_at_of(&rule), Some(due + 3600.0));

        tick_book(&graph, &talked(), due + 3.0 * 3600.0 + 1.0).expect("a tick");
        let rule = Records::new(&graph, RULE)
            .get(&talked(), "rule-1")
            .expect("the rule");
        assert_eq!(
            due_at_of(&rule),
            Some(due + 4.0 * 3600.0),
            "missed periods are skipped, not replayed one at a time"
        );
        // two dues were worked, and no more
        assert_eq!(
            Records::new(&graph, EXECUTION)
                .list(&talked(), 10)
                .expect("the runs")
                .len(),
            2
        );
    }

    #[test]
    fn a_run_lands_in_the_owners_inbox() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        rule_with(&graph, due, None);
        tick_book(&graph, &talked(), due).expect("a tick");

        let inbox = notifications::notifications::inbox_of(&graph, &talked(), "user-1", &json!({}))
            .expect("the inbox");
        let notices = inbox["notifications"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(notices.len(), 1, "one notice for one run: {inbox}");
        assert_eq!(notices[0]["type"], json!("workflow_completed"));
        assert!(
            text(&notices[0], "message").contains("Month end sweep"),
            "the notice names the rule that ran: {inbox}"
        );
    }

    #[test]
    fn a_rule_that_is_off_is_not_run_even_when_it_is_due() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        rule_with(&graph, due, None);
        // a rule starts on, so one toggle is what turns it off
        let flipped = automation::automations::toggle_automation(&graph, &talked(), "rule-1")
            .expect("a toggle");
        assert_eq!(flipped["enabled"], json!(false), "{flipped}");
        assert!(tick_book(&graph, &talked(), due)
            .expect("a tick")
            .is_empty());
    }

    #[test]
    fn a_rule_that_a_person_starts_manually_is_not_the_workers_to_start() {
        let graph = MemoryGraph::default();
        automation::automations::create_automation_rule(
            &graph,
            &talked(),
            &json!({
                "id": "rule-manual",
                "trigger": "manual",
                "condition": { "schedule": { "due_at": 1_763_212_800.0 } },
                "steps": [],
            }),
        )
        .expect("a rule");
        assert!(tick_book(&graph, &talked(), 1_763_212_900.0)
            .expect("a tick")
            .is_empty());
    }

    #[test]
    fn a_failing_step_is_recorded_as_a_failed_run_and_still_told_to_the_owner() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        automation::automations::create_automation_rule(
            &graph,
            &talked(),
            &json!({
                "id": "rule-broken",
                "name": "Broken sweep",
                "trigger": "scheduled",
                "condition": { "schedule": { "due_at": due } },
                // the second step depends on a step that never ran, so the walk refuses it
                "steps": [
                    { "step_id": "close", "action": "close_period", "depends_on": ["collect"] }
                ],
            }),
        )
        .expect("a rule");

        let answered = tick_book(&graph, &talked(), due).expect("a tick");
        assert_eq!(answered[0]["status"], json!("failed"), "{answered:?}");
        let runs = Records::new(&graph, EXECUTION)
            .list(&talked(), 10)
            .expect("the runs");
        assert_eq!(runs[0]["status"], json!("failed"));
        assert!(
            text(&runs[0], "error").contains("Dependency collect"),
            "the run says why it stopped: {runs:?}"
        );

        let inbox = notifications::notifications::inbox_of(&graph, &talked(), "user-1", &json!({}))
            .expect("the inbox");
        assert_eq!(inbox["total_count"], json!(1), "{inbox}");
        assert_eq!(inbox["notifications"][0]["type"], json!("workflow_failed"));
    }

    #[test]
    fn a_tick_works_every_book_in_the_store() {
        let graph = MemoryGraph::default();
        let due = 1_763_212_800.0;
        // two Books, each with its own due rule
        for (user, book, rule) in [
            ("user-1", "book-1", "rule-a"),
            ("user-2", "book-2", "rule-b"),
        ] {
            store::PlatformRecords::new(&graph, offline_sync::sync::BOOK)
                .create(&json!({ "id": book, "name": book, "created_by": user }))
                .expect("a book");
            automation::automations::create_automation_rule(
                &graph,
                &BookScope::new(user, book),
                &json!({
                    "id": rule,
                    "trigger": "scheduled",
                    "condition": { "schedule": { "due_at": due } },
                    "steps": [{ "step_id": "s1", "action": "noop" }],
                }),
            )
            .expect("a rule");
        }

        let answered = tick(&graph, due).expect("a tick");
        let mut ids = run_ids(&answered);
        ids.sort();
        assert_eq!(
            ids,
            vec!["run-rule-a-1763212800", "run-rule-b-1763212800"],
            "one run per Book"
        );

        // and a second pass finds both done
        let again = tick(&graph, due + 5.0).expect("a tick");
        for run in again {
            assert_eq!(run["outcome"], json!("already recorded"), "{run}");
        }
    }
}
