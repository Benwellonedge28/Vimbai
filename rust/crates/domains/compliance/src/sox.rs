#![forbid(unsafe_code)]
//! sox-compliance-service: the controls a Book registers, the tests run against them, and the deficiencies those
//! tests raise.
//!
//! Records are durable and Book-scoped, one label per kind under its own edge from the caller.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a control is registered active at medium risk unless the caller says otherwise, and carries the reference it
//!   was given (the caller's own numbering, e.g. SOX-ITGC-001) beside the record's own id;
//! * a test is only recorded against a control that exists and is the caller's, and the verdict is derived rather
//!   than taken from the caller: no exceptions is a pass, fewer than a tenth of the sample in exception is a pass
//!   with exception, and a tenth or more is a failure - so a tenth exactly fails, because the shipped comparison
//!   was `exceptions < sample * 0.1`;
//! * a failing test raises a deficiency by itself, before anyone asks: significant when more than a fifth of the
//!   sample failed, an ordinary control deficiency otherwise, with the plan left as `TBD`;
//! * a deficiency states one of three severities and any other word is refused, so the register cannot fill with
//!   near-misses;
//! * marking a deficiency remediated stamps the date it happened, and the date is the caller's, not the server's;
//! * the dashboard counts the control statuses and test verdicts over what the Book can see, and counts a material
//!   weakness as open until it is remediated - a remediated material weakness is not counted at all.

use serde_json::{json, Value};

use vimbai_store::read::{field, flag, refused, text, whole};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, SOX_CONTROL, SOX_CONTROL_TEST, SOX_DEFICIENCY,
};

/// The severities a deficiency may state.
const SEVERITIES: &[&str] = &[
    "control_deficiency",
    "significant_deficiency",
    "material_weakness",
];

fn today(payload: &Value) -> String {
    text(payload, "today")
}

fn or_stated(record: &Value, key: &str, fallback: &str) -> Value {
    match record.get(key) {
        Some(Value::Null) | None => json!(fallback),
        Some(held) => held.clone(),
    }
}

fn control_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "control_id_ref": field(record, "control_id_ref"),
        "description": field(record, "description"),
        "control_type": field(record, "control_type"),
        "control_nature": field(record, "control_nature"),
        "frequency": field(record, "frequency"),
        "owner": field(record, "owner"),
        "process": field(record, "process"),
        "risk_level": or_stated(record, "risk_level", "medium"),
        "status": or_stated(record, "status", "active"),
        "created_at": field(record, "created_at"),
    })
}

fn test_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "control_id": field(record, "control_id"),
        "test_period": field(record, "test_period"),
        "tester": field(record, "tester"),
        "sample_size": whole(record, "sample_size"),
        "exceptions_found": whole(record, "exceptions_found"),
        "result": or_stated(record, "result", "pass"),
        "test_date": field(record, "test_date"),
        "notes": field(record, "notes"),
    })
}

fn deficiency_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "control_id": field(record, "control_id"),
        "severity": field(record, "severity"),
        "description": field(record, "description"),
        "remediation_plan": field(record, "remediation_plan"),
        "remediation_owner": field(record, "remediation_owner"),
        "status": or_stated(record, "status", "open"),
        "identified_date": field(record, "identified_date"),
        "remediated_date": record.get("remediated_date").cloned().unwrap_or(Value::Null),
    })
}

/// The verdict a test earned, derived from the exceptions rather than taken from the caller.
pub fn verdict(exceptions: u32, sample: u32) -> &'static str {
    if exceptions == 0 {
        "pass"
    } else if (exceptions as f64) < (sample as f64) * 0.1 {
        // the shipped comparison was `< sample * 0.1`, so exactly a tenth fails
        "pass_with_exception"
    } else {
        "fail"
    }
}

/// The severity a failing test earns: significant when more than a fifth of the sample failed.
pub fn severity_for(exceptions: u32, sample: u32) -> &'static str {
    if (exceptions as f64) > (sample as f64) * 0.2 {
        "significant_deficiency"
    } else {
        "control_deficiency"
    }
}

fn all_controls(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, SOX_CONTROL).list(scope, 500)?;
    Ok(held.iter().map(control_of).collect())
}

fn all_tests(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, SOX_CONTROL_TEST).list(scope, 500)?;
    Ok(held.iter().map(test_of).collect())
}

fn all_deficiencies(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, SOX_DEFICIENCY).list(scope, 500)?;
    Ok(held.iter().map(deficiency_of).collect())
}

/// `POST /controls`: register a control.
pub fn create_control(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let risk_level = {
        let stated = text(payload, "risk_level");
        if stated.is_empty() {
            "medium".to_string()
        } else {
            stated
        }
    };
    let mut fields = json!({
        "control_id_ref": text(payload, "control_id_ref"),
        "description": text(payload, "description"),
        "control_type": text(payload, "control_type"),
        "control_nature": text(payload, "control_nature"),
        "frequency": text(payload, "frequency"),
        "owner": text(payload, "owner"),
        "process": text(payload, "process"),
        "risk_level": risk_level,
        "status": "active",
        "created_at": today(payload),
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, SOX_CONTROL).create(scope, &fields)?;
    Ok(control_of(&created))
}

/// `GET /controls`: the controls, narrowed to one process or one status when the caller says.
pub fn list_controls(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let process = text(query, "process");
    let status = text(query, "status");
    let mut mine = all_controls(graph, scope)?;
    if !process.is_empty() {
        mine.retain(|row| field(row, "process") == process);
    }
    if !status.is_empty() {
        mine.retain(|row| field(row, "status") == status);
    }
    Ok(Value::Array(mine))
}

/// `POST /controls/{id}/test`: record a test, and raise a deficiency when it failed.
pub fn test_control(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    control_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    // a test is only recorded against a control that exists and is the caller's
    Records::new(graph, SOX_CONTROL).get(scope, control_id)?;
    let sample = match whole(payload, "sample_size") {
        0 => 25,
        stated => stated,
    };
    let exceptions = whole(payload, "exceptions_found");
    let result = verdict(exceptions, sample);
    let mut fields = json!({
        "control_id": control_id,
        "test_period": text(payload, "test_period"),
        "tester": text(payload, "tester"),
        "sample_size": sample,
        "exceptions_found": exceptions,
        "result": result,
        "test_date": today(payload),
        "notes": text(payload, "notes"),
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, SOX_CONTROL_TEST).create(scope, &fields)?;

    if result == "fail" {
        // a failing test raises its own deficiency rather than waiting to be noticed
        let raised = json!({
            "control_id": control_id,
            "severity": severity_for(exceptions, sample),
            "description": format!(
                "Control test failed with {exceptions} exceptions out of {sample} samples."
            ),
            "remediation_plan": "TBD",
            "remediation_owner": "",
            "status": "open",
            "identified_date": today(payload),
            "remediated_date": Value::Null,
        });
        Records::new(graph, SOX_DEFICIENCY).create(scope, &raised)?;
    }
    Ok(test_of(&created))
}

/// `GET /controls/{id}/tests`: the tests run against one control.
pub fn list_tests(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    control_id: &str,
) -> Result<Value, StoreError> {
    let mut mine = all_tests(graph, scope)?;
    mine.retain(|row| field(row, "control_id") == control_id);
    Ok(Value::Array(mine))
}

/// `POST /deficiencies`: record a deficiency by hand.
pub fn create_deficiency(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let severity = text(payload, "severity");
    if !SEVERITIES.contains(&severity.as_str()) {
        return Err(refused(&format!(
            "Invalid severity. Must be one of {SEVERITIES:?}"
        )));
    }
    let mut fields = json!({
        "control_id": text(payload, "control_id"),
        "severity": severity,
        "description": text(payload, "description"),
        "remediation_plan": text(payload, "remediation_plan"),
        "remediation_owner": text(payload, "remediation_owner"),
        "status": "open",
        "identified_date": today(payload),
        "remediated_date": Value::Null,
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, SOX_DEFICIENCY).create(scope, &fields)?;
    Ok(deficiency_of(&created))
}

/// `GET /deficiencies`: the deficiencies, narrowed to one status when the caller says.
pub fn list_deficiencies(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let status = text(query, "status");
    let mut mine = all_deficiencies(graph, scope)?;
    if !status.is_empty() {
        mine.retain(|row| field(row, "status") == status);
    }
    Ok(Value::Array(mine))
}

/// `PUT /deficiencies/{id}`: move a deficiency on, stamping the date it was remediated.
pub fn update_deficiency(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    deficiency_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, SOX_DEFICIENCY).get(scope, deficiency_id)?;
    let status = {
        let stated = text(payload, "status");
        if stated.is_empty() {
            field(&held, "status").to_string()
        } else {
            stated
        }
    };
    let mut changed = json!({ "status": status });
    let plan = text(payload, "remediation_plan");
    if !plan.is_empty() {
        // an empty plan leaves the existing one alone, as shipped
        changed["remediation_plan"] = json!(plan);
    }
    if status == "remediated" {
        changed["remediated_date"] = json!(today(payload));
    }
    let saved = Records::new(graph, SOX_DEFICIENCY).update(scope, deficiency_id, &changed)?;
    Ok(deficiency_of(&saved))
}

/// `GET /dashboard`: the counts a compliance lead asks for.
pub fn sox_dashboard(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let controls = all_controls(graph, scope)?;
    let tests = all_tests(graph, scope)?;
    let deficiencies = all_deficiencies(graph, scope)?;
    let counted = |rows: &[Value], key: &str, wanted: &str| {
        rows.iter().filter(|row| field(row, key) == wanted).count()
    };
    let material_weaknesses = deficiencies
        .iter()
        .filter(|row| {
            field(row, "severity") == "material_weakness" && field(row, "status") != "remediated"
        })
        .count();
    Ok(json!({
        "total_controls": controls.len(),
        "active_controls": counted(&controls, "status", "active"),
        "total_tests": tests.len(),
        "passing_tests": counted(&tests, "result", "pass"),
        "failing_tests": counted(&tests, "result", "fail"),
        "open_deficiencies": counted(&deficiencies, "status", "open"),
        "material_weaknesses": material_weaknesses,
    }))
}

/// Whether a record is active, which the dashboard's counts rest on.
pub fn is_active(record: &Value) -> bool {
    flag(record, "active")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_control(graph: &MemoryGraph, scope: &BookScope, process: &str) -> Value {
        create_control(
            graph,
            scope,
            &json!({
                "control_id_ref": "SOX-ITGC-001",
                "description": "Access reviews are performed each quarter",
                "control_type": "detective",
                "control_nature": "manual",
                "frequency": "quarterly",
                "owner": "Head of IT",
                "process": process,
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .expect("a control")
    }

    #[test]
    fn a_control_is_active_at_medium_risk_and_keeps_the_reference_it_was_given() {
        let graph = MemoryGraph::new();
        let created = a_control(&graph, &scope(), "IT");
        assert_eq!(created["status"], json!("active"));
        assert_eq!(created["risk_level"], json!("medium"));
        assert_eq!(created["control_id_ref"], json!("SOX-ITGC-001"));
        assert!(!field(&created, "id").is_empty());
    }

    #[test]
    fn the_verdict_is_derived_and_a_tenth_exactly_fails() {
        assert_eq!(verdict(0, 25), "pass");
        assert_eq!(verdict(2, 25), "pass_with_exception");
        // two out of twenty is exactly a tenth, which the shipped comparison called a failure
        assert_eq!(verdict(2, 20), "fail");
        assert_eq!(verdict(6, 25), "fail");
    }

    #[test]
    fn a_severity_is_earned_by_how_much_of_the_sample_failed() {
        assert_eq!(severity_for(3, 25), "control_deficiency");
        assert_eq!(severity_for(6, 25), "significant_deficiency");
    }

    #[test]
    fn a_failing_test_raises_its_own_deficiency() {
        let graph = MemoryGraph::new();
        let control = a_control(&graph, &scope(), "IT");
        let tested = test_control(
            &graph,
            &scope(),
            field(&control, "id"),
            &json!({
                "test_period": "2026-Q1",
                "tester": "Internal audit",
                "sample_size": 25,
                "exceptions_found": 3,
                "today": "2026-04-01T09:00:00Z"
            }),
        )
        .expect("a test");
        assert_eq!(tested["result"], json!("fail"));
        assert_eq!(tested["sample_size"], json!(25));

        let raised = list_deficiencies(&graph, &scope(), &json!({})).expect("a listing");
        let rows = raised.as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["severity"], json!("control_deficiency"));
        assert_eq!(rows[0]["remediation_plan"], json!("TBD"));
        assert_eq!(
            rows[0]["description"],
            json!("Control test failed with 3 exceptions out of 25 samples.")
        );
    }

    #[test]
    fn a_passing_test_raises_nothing() {
        let graph = MemoryGraph::new();
        let control = a_control(&graph, &scope(), "IT");
        test_control(
            &graph,
            &scope(),
            field(&control, "id"),
            &json!({
                "test_period": "2026-Q1",
                "tester": "Internal audit",
                "exceptions_found": 1,
                "today": "2026-04-01T09:00:00Z"
            }),
        )
        .expect("a test");
        assert_eq!(
            list_deficiencies(&graph, &scope(), &json!({}))
                .expect("a listing")
                .as_array()
                .map(|rows| rows.len()),
            Some(0)
        );
    }

    #[test]
    fn a_test_needs_a_control_that_exists_and_is_mine() {
        let graph = MemoryGraph::new();
        let missing = test_control(
            &graph,
            &scope(),
            "no-such-control",
            &json!({"test_period": "2026-Q1", "tester": "me"}),
        );
        assert!(matches!(missing, Err(StoreError::NotFound)));

        let theirs = BookScope::new("user-2", "book-2");
        let other = a_control(&graph, &theirs, "IT");
        let not_mine = test_control(
            &graph,
            &scope(),
            field(&other, "id"),
            &json!({"test_period": "2026-Q1", "tester": "me"}),
        );
        assert!(not_mine.is_err());
    }

    #[test]
    fn a_deficiency_states_one_of_three_severities() {
        let graph = MemoryGraph::new();
        let refused_severity = create_deficiency(
            &graph,
            &scope(),
            &json!({
                "control_id": "C-1",
                "severity": "minor",
                "description": "Something small",
                "today": "2026-04-01T09:00:00Z"
            }),
        );
        assert!(refused_severity.is_err());
        let created = create_deficiency(
            &graph,
            &scope(),
            &json!({
                "control_id": "C-1",
                "severity": "material_weakness",
                "description": "Segregation of duties is absent",
                "today": "2026-04-01T09:00:00Z"
            }),
        )
        .expect("a deficiency");
        assert_eq!(created["status"], json!("open"));
        assert_eq!(created["identified_date"], json!("2026-04-01T09:00:00Z"));
    }

    #[test]
    fn remediating_stamps_the_date_and_keeps_the_plan_unless_a_new_one_is_stated() {
        let graph = MemoryGraph::new();
        let created = create_deficiency(
            &graph,
            &scope(),
            &json!({
                "control_id": "C-1",
                "severity": "control_deficiency",
                "description": "Late review",
                "remediation_plan": "Quarterly calendar",
                "today": "2026-04-01T09:00:00Z"
            }),
        )
        .expect("a deficiency");
        let moved = update_deficiency(
            &graph,
            &scope(),
            field(&created, "id"),
            &json!({"status": "remediated", "today": "2026-05-01T10:00:00Z"}),
        )
        .expect("an update");
        assert_eq!(moved["status"], json!("remediated"));
        assert_eq!(moved["remediated_date"], json!("2026-05-01T10:00:00Z"));
        assert_eq!(moved["remediation_plan"], json!("Quarterly calendar"));
    }

    #[test]
    fn the_dashboard_counts_a_material_weakness_until_it_is_remediated() {
        let graph = MemoryGraph::new();
        let control = a_control(&graph, &scope(), "IT");
        let weak = create_deficiency(
            &graph,
            &scope(),
            &json!({
                "control_id": field(&control, "id"),
                "severity": "material_weakness",
                "description": "No segregation of duties",
                "today": "2026-04-01T09:00:00Z"
            }),
        )
        .expect("a deficiency");
        let before = sox_dashboard(&graph, &scope()).expect("a dashboard");
        assert_eq!(before["material_weaknesses"], json!(1));
        assert_eq!(before["open_deficiencies"], json!(1));

        update_deficiency(
            &graph,
            &scope(),
            field(&weak, "id"),
            &json!({"status": "remediated", "today": "2026-05-01T10:00:00Z"}),
        )
        .expect("an update");
        let after = sox_dashboard(&graph, &scope()).expect("a dashboard");
        // a remediated material weakness is not counted at all
        assert_eq!(after["material_weaknesses"], json!(0));
        assert_eq!(after["open_deficiencies"], json!(0));
        assert_eq!(after["active_controls"], json!(1));
    }

    #[test]
    fn the_controls_listing_narrows_by_process_and_status() {
        let graph = MemoryGraph::new();
        a_control(&graph, &scope(), "IT");
        a_control(&graph, &scope(), "Finance");
        let mine = list_controls(&graph, &scope(), &json!({"process": "IT"})).expect("a listing");
        assert_eq!(mine.as_array().expect("rows").len(), 1);
        let none =
            list_controls(&graph, &scope(), &json!({"status": "retired"})).expect("a listing");
        assert_eq!(none.as_array().expect("rows").len(), 0);
        assert_eq!(
            list_controls(&graph, &BookScope::new("user-2", "book-2"), &json!({}))
                .expect("a listing")
                .as_array()
                .map(|rows| rows.len()),
            Some(0)
        );
    }
}
