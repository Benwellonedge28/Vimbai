#![forbid(unsafe_code)]
//! automation-engine-service: the rules a Book automates, and what happened when one ran.
//!
//! A rule is a name, a trigger and an ordered list of steps; running it walks those steps and writes down what each one
//! did. The port keeps the two behaviours that decide whether a run is trustworthy:
//!
//! * a step that depends on a step which has not completed stops the run. The execution is marked failed with the
//!   dependency and the step named in the error, and no later step is attempted. Running the tail of a workflow after a
//!   step it relied on never happened is how a Book ends up with half its automation applied, so it is refused outright;
//! * a rule that is switched off refuses to run. Asking a disabled rule to run is a bad request, not a silent no-op,
//!   because a caller who believes a run happened will read the empty result as "the rule had nothing to do".
//!
//! Two further points, both kept as shipped rather than tidied:
//!
//! * deleting a rule that is not there answers `deleted: false` rather than missing. The caller is told the rule is gone
//!   either way, and the identifier comes back with the answer, so a delete can be repeated safely;
//! * steps do not actually perform anything here. This service records that a step was taken and what parameters it
//!   carried; the work itself belongs to whichever service the action names. Claiming a step did something would be the
//!   fiction to avoid, which is why every step result says `completed` for the step's passage and carries its parameters
//!   for the record.
//!
//! One difference from the shipped service is deliberate: the run is performed inside the request rather than handed to a
//! background task, so the answer is the finished execution instead of a `running` one the caller has to poll for. A
//! caller that polled still can (`GET /executions/{id}` answers the same record); a caller that read the first answer
//! used to learn nothing, and no longer does.

use crate::handler::{ActionRequest, StepHandler};
use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{flag, number, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A rule a Book automates.
pub const RULE: RecordKind = RecordKind::new("AutomationRule", "OWNS_AUTOMATION_RULE");
/// One run of a rule.
pub const EXECUTION: RecordKind = RecordKind::new("WorkflowExecution", "OWNS_WORKFLOW_EXECUTION");

/// The four ways an automation can be started. Anything else is refused by name.
pub const TRIGGERS: [&str; 4] = ["scheduled", "event", "manual", "webhook"];

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `POST /rules`: record a rule.
pub fn create_automation_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let trigger = match text(payload, "trigger").as_str() {
        "" => "manual".to_string(),
        named if TRIGGERS.contains(&named) => named.to_string(),
        _ => {
            return Err(StoreError::BadRequest(format!(
                "Invalid trigger. Must be one of {TRIGGERS:?}"
            )))
        }
    };
    let steps = payload
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Records::new(graph, RULE).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "name": text(payload, "name"),
            "company_id": text(payload, "company_id"),
            "trigger": trigger,
            "condition": payload.get("condition").cloned().unwrap_or_else(|| json!({})),
            "steps": steps,
            // a rule is on unless the caller says otherwise
            "enabled": payload.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            "priority": match payload.get("priority") {
                Some(_) => number(payload, "priority") as i64,
                None => 5,
            },
        }),
    )
}

/// `GET /rules`: the Book's rules, narrowed by company when asked.
pub fn automations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "company_id");
    let rows = Records::new(graph, RULE).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| wanted.is_empty() || text(row, "company_id") == wanted)
            .collect(),
    ))
}

/// `GET /rules/{id}`: one rule.
pub fn automation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, RULE).get(scope, rule_id)
}

/// `DELETE /rules/{id}`: forget a rule, answering whether there was one to forget.
pub fn delete_automation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    let deleted = Records::new(graph, RULE).delete(scope, rule_id).is_ok();
    Ok(json!({"deleted": deleted, "rule_id": rule_id}))
}

/// `POST /rules/{id}/toggle`: flip a rule on or off.
pub fn toggle_automation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    let held = automation(graph, scope, rule_id)?;
    let enabled = !flag(&held, "enabled");
    Records::new(graph, RULE).update(scope, rule_id, &json!({"enabled": enabled}))?;
    Ok(json!({"rule_id": rule_id, "enabled": enabled}))
}

/// What running a rule's steps came to.
///
/// Split out from the store call because it is the part worth testing on its own: given the steps in order, either the
/// run completes or it fails naming the dependency it was waiting for.
pub struct RunOutcome {
    /// One record per step that was taken, in order.
    pub steps: Vec<Value>,
    /// Why the run stopped, when it did.
    pub error: Option<String>,
}

/// Walk the steps of a rule, refusing to run a step whose dependency has not completed.
///
/// Every step is handed to `handler`, which is what actually does the work. A step that is refused stops the run: what
/// was done is kept, and the reason is recorded, so a person sees the sentence rather than a rule that looks finished.
pub fn walk_steps(steps: &[Value], handler: &impl StepHandler, run_id: &str) -> RunOutcome {
    let mut done: Vec<String> = Vec::new();
    let mut results = Vec::with_capacity(steps.len());
    for step in steps {
        for dependency in string_list(step.get("depends_on")) {
            if !done.contains(&dependency) {
                return RunOutcome {
                    steps: results,
                    error: Some(format!(
                        "Dependency {dependency} not completed for step {}",
                        text(step, "step_id")
                    )),
                };
            }
        }
        let step_id = text(step, "step_id");
        let action = text(step, "action");
        let params = step.get("params").cloned().unwrap_or_else(|| json!({}));
        let request = ActionRequest {
            run_id,
            step_id: &step_id,
            action: &action,
            params: &params,
        };
        match handler.act(&request) {
            Ok(result) => {
                results.push(json!({
                    "step_id": step_id,
                    "step_name": text(step, "step_name"),
                    "action": action,
                    "status": "completed",
                    "params": params,
                    "result": result,
                }));
                done.push(step_id);
            }
            Err(refusal) => {
                results.push(json!({
                    "step_id": step_id,
                    "step_name": text(step, "step_name"),
                    "action": action,
                    "status": "refused",
                    "params": params,
                    "reason": refusal,
                }));
                return RunOutcome {
                    steps: results,
                    error: Some(format!(
                        "Step {} could not be taken: {refusal}",
                        text(step, "step_id")
                    )),
                };
            }
        }
    }
    RunOutcome {
        steps: results,
        error: None,
    }
}

/// `POST /execute/{id}`: run a rule, and answer the finished execution.
pub fn execute_automation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
    payload: &Value,
    handler: &impl StepHandler,
) -> Result<Value, StoreError> {
    let rule = automation(graph, scope, rule_id)?;
    if !flag(&rule, "enabled") {
        return Err(StoreError::BadRequest("Rule is disabled".to_string()));
    }
    let steps = rule
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let id = if text(payload, "id").is_empty() {
        service_id()
    } else {
        text(payload, "id")
    };
    let outcome = walk_steps(&steps, handler, &id);
    let started = stamp(payload);
    Records::new(graph, EXECUTION).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "rule_id": rule_id,
            "company_id": text(&rule, "company_id"),
            "status": if outcome.error.is_some() { "failed" } else { "completed" },
            "started_at": started,
            "completed_at": started,
            "step_results": outcome.steps,
            "error": outcome.error,
        }),
    )
}

/// `GET /executions`: the runs this Book made, narrowed by company and status when asked.
pub fn executions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted_company = text(payload, "company_id");
    let wanted_status = text(payload, "status");
    let rows = Records::new(graph, EXECUTION).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| wanted_company.is_empty() || text(row, "company_id") == wanted_company)
            .filter(|row| wanted_status.is_empty() || text(row, "status") == wanted_status)
            .collect(),
    ))
}

/// `GET /executions/{id}`: one run.
pub fn execution(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    execution_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, EXECUTION).get(scope, execution_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::NoActions;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A handler that does what it is asked and remembers what it was asked, so a test can prove a step reached it.
    #[derive(Default)]
    struct Recorder {
        seen: std::cell::RefCell<Vec<String>>,
        refuse: Option<String>,
    }

    impl StepHandler for Recorder {
        fn act(&self, request: &ActionRequest<'_>) -> Result<Value, String> {
            self.seen.borrow_mut().push(format!(
                "{}|{}|{}",
                request.action,
                request.step_id,
                request.reference()
            ));
            match &self.refuse {
                Some(reason) => Err(reason.clone()),
                None => Ok(json!({"action": request.action})),
            }
        }
    }

    fn recorder() -> Recorder {
        Recorder::default()
    }

    fn rule(graph: &MemoryGraph, steps: Value) -> Value {
        create_automation_rule(
            graph,
            &talked(),
            &json!({"name": "Month end sweep", "company_id": "co-1", "trigger": "scheduled",
                    "steps": steps}),
        )
        .expect("a rule")
    }

    #[test]
    fn a_run_walks_its_steps_until_a_dependency_is_missing() {
        // steps in order with their dependencies met: every step is taken and recorded
        let hands = recorder();
        let passing = walk_steps(
            &[
                json!({"step_id": "s1", "step_name": "Post accruals", "action": "post_journal",
                   "params": {"book": "main"}}),
                json!({"step_id": "s2", "step_name": "Reconcile", "action": "reconcile",
                   "depends_on": ["s1"]}),
            ],
            &hands,
            "run-1",
        );
        assert!(passing.error.is_none());
        assert_eq!(passing.steps.len(), 2);
        assert_eq!(passing.steps[1]["step_id"], json!("s2"));
        assert_eq!(passing.steps[0]["params"], json!({"book": "main"}));
        assert_eq!(passing.steps[1]["status"], json!("completed"));

        // a step waiting on one that never completed stops the run and names both
        let failing = walk_steps(
            &[
                json!({"step_id": "s1", "step_name": "Post accruals", "action": "post_journal"}),
                json!({"step_id": "s2", "step_name": "Report", "action": "report",
                   "depends_on": ["s9"]}),
                json!({"step_id": "s3", "step_name": "Never reached", "action": "close"}),
            ],
            &hands,
            "run-1",
        );
        assert_eq!(
            failing.error.as_deref(),
            Some("Dependency s9 not completed for step s2")
        );
        // the step that ran is kept, and nothing after the refusal was attempted
        assert_eq!(failing.steps.len(), 1);
        assert_eq!(failing.steps[0]["step_id"], json!("s1"));
    }

    #[test]
    fn a_step_reaches_the_hands_it_is_given_and_carries_a_reference() {
        // the rule is a description; what happens is what the handler does. The reference is the run and the step, so a
        // posting made by a step can be refused by the ledger if the same due instant is walked twice.
        let hands = recorder();
        let outcome = walk_steps(
            &[
                json!({"step_id": "s1", "step_name": "Depreciate", "action": "post_depreciation",
                       "params": {"period": "2026-11"}}),
                json!({"step_id": "s2", "step_name": "Close", "action": "close_period",
                       "depends_on": ["s1"]}),
            ],
            &hands,
            "run-r7-1763212800",
        );
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        assert_eq!(
            hands.seen.borrow().as_slice(),
            [
                "post_depreciation|s1|run-r7-1763212800-s1",
                "close_period|s2|run-r7-1763212800-s2"
            ]
        );
        // what the handler answered is kept on the step, so a run says what it did and not only that it finished
        assert_eq!(
            outcome.steps[0]["result"],
            json!({"action": "post_depreciation"})
        );
    }

    #[test]
    fn a_step_the_hands_refuse_stops_the_run_and_keeps_the_sentence() {
        // a rule that names something nothing can do must not look finished: the run stops, says which step, and says
        // why, because a rule that quietly did nothing is indistinguishable from one that worked
        let hands = Recorder {
            refuse: Some(
                "No asset is registered yet, so there is nothing to depreciate.".to_string(),
            ),
            ..Recorder::default()
        };
        let outcome = walk_steps(
            &[
                json!({"step_id": "s1", "step_name": "Depreciate", "action": "post_depreciation"}),
                json!({"step_id": "s2", "step_name": "Close", "action": "close_period",
                       "depends_on": ["s1"]}),
            ],
            &hands,
            "run-1",
        );
        assert_eq!(
            outcome.error.as_deref(),
            Some("Step s1 could not be taken: No asset is registered yet, so there is nothing to depreciate.")
        );
        assert_eq!(outcome.steps.len(), 1);
        assert_eq!(outcome.steps[0]["status"], json!("refused"));
        assert_eq!(
            outcome.steps[0]["reason"],
            json!("No asset is registered yet, so there is nothing to depreciate.")
        );
        // the step after it was never attempted
        assert_eq!(hands.seen.borrow().len(), 1);

        // a runner with no hands is honest about it rather than reporting success
        let nothing = walk_steps(
            &[json!({"step_id": "s1", "step_name": "Depreciate", "action": "post_depreciation"})],
            &NoActions,
            "run-1",
        );
        assert!(nothing.error.is_some());
        assert!(nothing.steps[0]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("Nothing here can do"));
    }

    #[test]
    fn a_rule_runs_only_while_it_is_switched_on() {
        let graph = MemoryGraph::new();
        let made = rule(
            &graph,
            json!([
                {"step_id": "s1", "step_name": "Post accruals", "action": "post_journal"},
                {"step_id": "s2", "step_name": "Reconcile", "action": "reconcile", "depends_on": ["s1"]}
            ]),
        );
        let id = text(&made, "id");
        assert_eq!(made["enabled"], json!(true));
        assert_eq!(made["trigger"], json!("scheduled"));
        assert_eq!(made["priority"], json!(5));

        let run = execute_automation(&graph, &talked(), &id, &json!({"now": 2000.0}), &recorder())
            .expect("a run");
        assert_eq!(run["status"], json!("completed"));
        assert_eq!(run["rule_id"], json!(id));
        assert_eq!(run["company_id"], json!("co-1"));
        assert_eq!(run["started_at"], run["completed_at"]);
        assert_eq!(run["step_results"].as_array().map(Vec::len), Some(2));
        assert_eq!(run["error"], Value::Null);

        // switching off refuses the run, so a caller never reads an empty run as "nothing to do"
        let off = toggle_automation(&graph, &talked(), &id).expect("a toggle");
        assert_eq!(off["enabled"], json!(false));
        assert!(matches!(
            execute_automation(&graph, &talked(), &id, &json!({}), &recorder()),
            Err(StoreError::BadRequest(_))
        ));
        let on = toggle_automation(&graph, &talked(), &id).expect("a toggle");
        assert_eq!(on["enabled"], json!(true));

        // a rule that cannot complete is recorded as failed, with the reason kept on the run
        let broken = rule(
            &graph,
            json!([{"step_id": "s1", "step_name": "Report", "action": "report", "depends_on": ["gone"]}]),
        );
        let failed = execute_automation(
            &graph,
            &talked(),
            &text(&broken, "id"),
            &json!({}),
            &recorder(),
        )
        .expect("a run");
        assert_eq!(failed["status"], json!("failed"));
        assert_eq!(
            failed["error"],
            json!("Dependency gone not completed for step s1")
        );
        assert_eq!(failed["step_results"].as_array().map(Vec::len), Some(0));

        // the listing narrows by company and by status, and a run is readable by id
        let all = executions(&graph, &talked(), &json!({})).expect("runs");
        assert_eq!(all.as_array().map(Vec::len), Some(2));
        let failed_only =
            executions(&graph, &talked(), &json!({"status": "failed"})).expect("runs");
        assert_eq!(failed_only.as_array().map(Vec::len), Some(1));
        let other_company =
            executions(&graph, &talked(), &json!({"company_id": "co-2"})).expect("runs");
        assert_eq!(other_company.as_array().map(Vec::len), Some(0));
        let one = execution(&graph, &talked(), &text(&run, "id")).expect("a run");
        assert_eq!(one["id"], run["id"]);

        // another Book sees, runs and reads none of it
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(execute_automation(&graph, &elsewhere, &id, &json!({}), &recorder()).is_err());
        assert!(execution(&graph, &elsewhere, &text(&run, "id")).is_err());
        assert_eq!(
            automations(&graph, &elsewhere, &json!({}))
                .expect("rules")
                .as_array()
                .map(Vec::len),
            Some(0)
        );

        // a trigger nobody agreed on is a bad request, while an unstated one is a manual rule
        assert!(matches!(
            create_automation_rule(
                &graph,
                &talked(),
                &json!({"name": "x", "trigger": "whenever"})
            ),
            Err(StoreError::BadRequest(_))
        ));
        let manual =
            create_automation_rule(&graph, &talked(), &json!({"name": "By hand"})).expect("a rule");
        assert_eq!(manual["trigger"], json!("manual"));
        assert_eq!(manual["priority"], json!(5));
    }

    #[test]
    fn deleting_is_safe_to_repeat_and_answers_what_it_found() {
        let graph = MemoryGraph::new();
        let made = rule(&graph, json!([]));
        let id = text(&made, "id");
        let gone = delete_automation(&graph, &talked(), &id).expect("a delete");
        assert_eq!(gone["deleted"], json!(true));
        assert_eq!(gone["rule_id"], json!(id));
        // the shipped service answered deleted: false rather than missing, and the identifier still comes back
        let again = delete_automation(&graph, &talked(), &id).expect("a delete");
        assert_eq!(again["deleted"], json!(false));
        assert!(automation(&graph, &talked(), &id).is_err());
        assert!(toggle_automation(&graph, &talked(), &id).is_err());
    }
}
