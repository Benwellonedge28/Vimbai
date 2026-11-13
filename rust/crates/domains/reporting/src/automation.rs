#![forbid(unsafe_code)]
//! report-automation-service: the report templates a Book keeps, the schedules they run on, and the reports those
//! schedules have produced.
//!
//! Records are durable and Book-scoped: a template, a schedule and a generated report are three record kinds, each
//! held under its own label and its own edge from the caller, so what one Book asked for is invisible to another.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a template states one of four report types, and anything else is refused rather than stored;
//! * a schedule must name a template that exists - without one it could never produce anything;
//! * a schedule starts active, and its next run is the moment it was recorded until something runs it;
//! * a report runs only from an active schedule, and running it is what moves the schedule's last run;
//! * a run copies the template's sections into the report, and addresses it to the schedule's recipients - so a
//!   schedule with recipients reports itself delivered and one without reports itself generated;
//! * a period is what the caller stated, and the month it was recorded in when the caller stated nothing;
//! * pausing and resuming are status changes, so nothing is lost by pausing: the reports already produced stay;
//! * a listing is the newest kept: the reports are the tail of the list, not the head.

use serde_json::{json, Value};

use vimbai_store::query::query_page;
use vimbai_store::read::{field, refused, text};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, GENERATED_REPORT, REPORT_SCHEDULE, REPORT_TEMPLATE,
};

/// What a template may be for.
const REPORT_TYPES: &[&str] = &[
    "financial_summary",
    "kpi_dashboard",
    "compliance",
    "operational",
];

/// The format a template states, defaulting to the one the shipped service defaulted to.
fn format_of(payload: &Value) -> String {
    let stated = text(payload, "format");
    if stated.is_empty() {
        "pdf".to_string()
    } else {
        stated
    }
}

/// A stated moment, or a refusal. Reckoning comes from the request rather than a clock, so the same request
/// answers the same thing.
fn moment(payload: &Value) -> Result<String, StoreError> {
    match payload.get("today").and_then(Value::as_str) {
        Some(stated) if !stated.trim().is_empty() => Ok(stated.trim().to_string()),
        _ => Err(refused("today is required")),
    }
}

/// The month a moment falls in, as `YYYY-MM`, for a run that states no period.
fn month_of(moment: &str) -> String {
    let digits: String = moment
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    let stamped: String = digits.chars().take(6).collect();
    if stamped.len() == 6 {
        format!("{}-{}", &stamped[..4], &stamped[4..])
    } else {
        stamped
    }
}

/// The lines a caller listed under one key, as strings.
fn lines(payload: &Value, key: &str) -> Vec<String> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|stated| {
            stated
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn state_of(record: &Value) -> String {
    let stated = field(record, "status");
    if stated.is_empty() {
        "active".to_string()
    } else {
        stated.to_string()
    }
}

fn or_stated(record: &Value, key: &str, fallback: &str) -> String {
    let stated = field(record, key);
    if stated.is_empty() {
        fallback.to_string()
    } else {
        stated.to_string()
    }
}

fn template_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "name": field(record, "name"),
        "report_type": field(record, "report_type"),
        "description": field(record, "description"),
        "sections": record.get("sections").cloned().unwrap_or_else(|| json!([])),
        "data_sources": record.get("data_sources").cloned().unwrap_or_else(|| json!([])),
        "format": or_stated(record, "format", "pdf"),
        "created_at": field(record, "created_at"),
    })
}

fn schedule_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "template_id": field(record, "template_id"),
        "name": field(record, "name"),
        "cron_expression": field(record, "cron_expression"),
        "recipients": record.get("recipients").cloned().unwrap_or_else(|| json!([])),
        "status": state_of(record),
        "last_run": record.get("last_run").cloned().unwrap_or(Value::Null),
        "next_run": record.get("next_run").cloned().unwrap_or(Value::Null),
        "created_at": field(record, "created_at"),
    })
}

fn report_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "schedule_id": field(record, "schedule_id"),
        "template_id": field(record, "template_id"),
        "period": field(record, "period"),
        "data": record.get("data").cloned().unwrap_or_else(|| json!({})),
        "status": or_stated(record, "status", "generated"),
        "generated_at": field(record, "generated_at"),
        "delivered_to": record.get("delivered_to").cloned().unwrap_or_else(|| json!([])),
    })
}

fn templates_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, REPORT_TEMPLATE).list(scope, 500)?;
    Ok(held.iter().map(template_of).collect())
}

fn schedules_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, REPORT_SCHEDULE).list(scope, 500)?;
    Ok(held.iter().map(schedule_of).collect())
}

fn reports_of(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, GENERATED_REPORT).list(scope, 500)?;
    Ok(held.iter().map(report_of).collect())
}

fn schedule_record(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    schedule_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, REPORT_SCHEDULE).get(scope, schedule_id)
}

/// `POST /templates`: a template, for one of the four report types.
pub fn create_template(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let report_type = text(payload, "report_type");
    if !REPORT_TYPES.contains(&report_type.as_str()) {
        return Err(refused(&format!(
            "Invalid report type. Must be one of {REPORT_TYPES:?}"
        )));
    }
    let mut fields = json!({
        "name": name,
        "report_type": report_type,
        "description": text(payload, "description"),
        "sections": lines(payload, "sections"),
        "data_sources": lines(payload, "data_sources"),
        "format": format_of(payload),
        "created_at": moment(payload)?,
    });
    // an id the caller states is honoured; otherwise the store mints one
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, REPORT_TEMPLATE).create(scope, &fields)?;
    Ok(template_of(&created))
}

/// `GET /templates`: the Book's templates, narrowed to one report type when the caller names it.
pub fn list_templates(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(query, "report_type");
    let mut mine = templates_of(graph, scope)?;
    if !wanted.is_empty() {
        mine.retain(|row| field(row, "report_type") == wanted);
    }
    Ok(json!({ "templates": mine }))
}

/// `POST /schedules`: a schedule for a template that exists.
pub fn create_schedule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let template_id = text(payload, "template_id");
    if template_id.is_empty() {
        return Err(refused("template_id is required"));
    }
    // a schedule for nothing could never produce anything, so the template has to be there
    Records::new(graph, REPORT_TEMPLATE).get(scope, &template_id)?;
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let recorded = moment(payload)?;
    let mut fields = json!({
        "template_id": template_id,
        "name": name,
        "cron_expression": text(payload, "cron_expression"),
        "recipients": lines(payload, "recipients"),
        "status": "active",
        "last_run": Value::Null,
        // nothing has run it yet, so its next run is the moment it was recorded
        "next_run": recorded,
        "created_at": recorded,
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, REPORT_SCHEDULE).create(scope, &fields)?;
    Ok(schedule_of(&created))
}

/// `GET /schedules`: the Book's schedules, narrowed to one state when the caller names it.
pub fn list_schedules(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(query, "status");
    let mut mine = schedules_of(graph, scope)?;
    if !wanted.is_empty() {
        mine.retain(|row| field(row, "status") == wanted);
    }
    Ok(json!({ "schedules": mine }))
}

/// `POST /schedules/{id}/run`: produce a report from a schedule, and move the schedule's last run.
pub fn run_report(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    schedule_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let schedule = schedule_record(graph, scope, schedule_id)?;
    if state_of(&schedule) != "active" {
        return Err(refused("Schedule is not active"));
    }
    let template_id = field(&schedule, "template_id").to_string();
    // a template that has since gone leaves the report with no sections rather than failing the run
    let sections = Records::new(graph, REPORT_TEMPLATE)
        .get(scope, &template_id)
        .map(|template| {
            template
                .get("sections")
                .cloned()
                .unwrap_or_else(|| json!([]))
        })
        .unwrap_or_else(|_| json!([]));
    let recipients = schedule
        .get("recipients")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let addressed = recipients
        .as_array()
        .map(|lines| !lines.is_empty())
        .unwrap_or(false);
    let recorded = moment(payload)?;
    let stated = text(payload, "period");
    let period = if stated.is_empty() {
        month_of(&recorded)
    } else {
        stated
    };
    let fields = json!({
        "schedule_id": schedule_id,
        "template_id": template_id,
        "period": period,
        "data": { "sections": sections },
        // a report addressed to somebody is delivered; one addressed to nobody is only generated
        "status": if addressed { "delivered" } else { "generated" },
        "generated_at": recorded,
        "delivered_to": recipients,
    });
    let created = Records::new(graph, GENERATED_REPORT).create(scope, &fields)?;
    // running it is what moves the schedule's last run
    Records::new(graph, REPORT_SCHEDULE).update(
        scope,
        schedule_id,
        &json!({ "last_run": recorded }),
    )?;
    Ok(report_of(&created))
}

/// `GET /reports`: the reports produced, narrowed to one schedule, and the newest kept.
pub fn list_generated_reports(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(query, "schedule_id");
    let kept = query_page(query, 50, 500);
    let mut mine = reports_of(graph, scope)?;
    if !wanted.is_empty() {
        mine.retain(|row| field(row, "schedule_id") == wanted);
    }
    // the shipped listing kept the tail of the list
    let first = mine.len().saturating_sub(kept);
    let kept_rows: Vec<Value> = mine.split_off(first);
    Ok(json!({ "reports": kept_rows }))
}

/// `PUT /schedules/{id}/pause`: a status change, so the reports it produced stay readable.
pub fn pause_schedule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    schedule_id: &str,
) -> Result<Value, StoreError> {
    schedule_record(graph, scope, schedule_id)?;
    Records::new(graph, REPORT_SCHEDULE).update(
        scope,
        schedule_id,
        &json!({ "status": "paused" }),
    )?;
    Ok(json!({ "schedule_id": schedule_id, "status": "paused" }))
}

/// `PUT /schedules/{id}/resume`: back to active.
pub fn resume_schedule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    schedule_id: &str,
) -> Result<Value, StoreError> {
    schedule_record(graph, scope, schedule_id)?;
    Records::new(graph, REPORT_SCHEDULE).update(
        scope,
        schedule_id,
        &json!({ "status": "active" }),
    )?;
    Ok(json!({ "schedule_id": schedule_id, "status": "active" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn template(graph: &MemoryGraph, scope: &BookScope) -> Value {
        create_template(
            graph,
            scope,
            &json!({
                "name": "Monthly pack",
                "report_type": "financial_summary",
                "sections": ["trial_balance", "cash_flow"],
                "today": "2026-03-01T09:00:00+00:00"
            }),
        )
        .expect("a template")
    }

    #[test]
    fn a_template_states_one_of_four_report_types_and_nothing_else() {
        let graph = MemoryGraph::new();
        let created = template(&graph, &scope());
        assert_eq!(created["report_type"], json!("financial_summary"));
        assert_eq!(created["sections"][0], json!("trial_balance"));
        assert_eq!(created["format"], json!("pdf"));

        let refused_type = create_template(
            &graph,
            &scope(),
            &json!({"name": "Odd", "report_type": "everything", "today": "2026-03-01T09:00:00"}),
        );
        assert!(refused_type.is_err());
        assert_eq!(templates_of(&graph, &scope()).unwrap().len(), 1);
    }

    #[test]
    fn a_schedule_needs_a_template_that_exists() {
        let graph = MemoryGraph::new();
        let missing = create_schedule(
            &graph,
            &scope(),
            &json!({
                "template_id": "no-such-template",
                "name": "Monthly",
                "cron_expression": "0 0 1 * *",
                "today": "2026-03-01T09:00:00Z"
            }),
        );
        assert!(matches!(missing, Err(StoreError::NotFound)));
    }

    #[test]
    fn a_schedule_starts_active_with_its_next_run_at_the_moment_it_was_recorded() {
        let graph = MemoryGraph::new();
        let template_id = field(&template(&graph, &scope()), "id").to_string();
        let created = create_schedule(
            &graph,
            &scope(),
            &json!({
                "template_id": template_id,
                "name": "Monthly",
                "cron_expression": "0 0 1 * *",
                "recipients": ["board@example.com"],
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .expect("a schedule");
        assert_eq!(created["status"], json!("active"));
        assert_eq!(created["next_run"], json!("2026-03-01T09:00:00Z"));
        assert_eq!(created["last_run"], Value::Null);
    }

    #[test]
    fn running_a_schedule_copies_the_template_and_moves_the_last_run() {
        let graph = MemoryGraph::new();
        let template_id = field(&template(&graph, &scope()), "id").to_string();
        let schedule = create_schedule(
            &graph,
            &scope(),
            &json!({
                "template_id": template_id,
                "name": "Monthly",
                "recipients": ["board@example.com"],
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .expect("a schedule");
        let schedule_id = field(&schedule, "id").to_string();
        let report = run_report(
            &graph,
            &scope(),
            &schedule_id,
            &json!({"today": "2026-04-01T08:00:00Z"}),
        )
        .expect("a report");
        assert_eq!(report["data"]["sections"][1], json!("cash_flow"));
        assert_eq!(report["delivered_to"][0], json!("board@example.com"));
        assert_eq!(report["status"], json!("delivered"));
        // the period is the month the run fell in, because the caller stated none
        assert_eq!(report["period"], json!("2026-04"));
        let after = schedule_record(&graph, &scope(), &schedule_id).expect("the schedule");
        assert_eq!(after["last_run"], json!("2026-04-01T08:00:00Z"));
    }

    #[test]
    fn a_schedule_addressing_nobody_generates_rather_than_delivers() {
        let graph = MemoryGraph::new();
        let template_id = field(&template(&graph, &scope()), "id").to_string();
        let schedule = create_schedule(
            &graph,
            &scope(),
            &json!({"template_id": template_id, "name": "Quiet", "today": "2026-03-01T09:00:00Z"}),
        )
        .expect("a schedule");
        let report = run_report(
            &graph,
            &scope(),
            field(&schedule, "id"),
            &json!({"today": "2026-03-02T09:00:00Z", "period": "2026-02"}),
        )
        .expect("a report");
        assert_eq!(report["status"], json!("generated"));
        assert_eq!(report["period"], json!("2026-02"));
        assert_eq!(report["delivered_to"], json!([]));
    }

    #[test]
    fn a_paused_schedule_does_not_run_and_says_so() {
        let graph = MemoryGraph::new();
        let template_id = field(&template(&graph, &scope()), "id").to_string();
        let schedule = create_schedule(
            &graph,
            &scope(),
            &json!({"template_id": template_id, "name": "Paused soon", "today": "2026-03-01T09:00:00Z"}),
        )
        .expect("a schedule");
        let schedule_id = field(&schedule, "id").to_string();
        pause_schedule(&graph, &scope(), &schedule_id).expect("a pause");
        let refused_run = run_report(
            &graph,
            &scope(),
            &schedule_id,
            &json!({"today": "2026-03-02T09:00:00Z"}),
        );
        assert!(refused_run.is_err());
        resume_schedule(&graph, &scope(), &schedule_id).expect("a resume");
        assert!(run_report(
            &graph,
            &scope(),
            &schedule_id,
            &json!({"today": "2026-03-02T09:00:00Z"})
        )
        .is_ok());
    }

    #[test]
    fn pausing_keeps_the_reports_the_schedule_already_produced() {
        let graph = MemoryGraph::new();
        let template_id = field(&template(&graph, &scope()), "id").to_string();
        let schedule = create_schedule(
            &graph,
            &scope(),
            &json!({"template_id": template_id, "name": "Runs once", "today": "2026-03-01T09:00:00Z"}),
        )
        .expect("a schedule");
        let schedule_id = field(&schedule, "id").to_string();
        run_report(
            &graph,
            &scope(),
            &schedule_id,
            &json!({"today": "2026-03-02T09:00:00Z"}),
        )
        .expect("a report");
        pause_schedule(&graph, &scope(), &schedule_id).expect("a pause");
        let listed = list_generated_reports(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(listed["reports"].as_array().expect("reports").len(), 1);
    }

    #[test]
    fn a_report_listing_keeps_the_newest_and_narrows_to_one_schedule() {
        let graph = MemoryGraph::new();
        let template_id = field(&template(&graph, &scope()), "id").to_string();
        let first = create_schedule(
            &graph,
            &scope(),
            &json!({"template_id": template_id, "name": "First", "today": "2026-03-01T09:00:00Z"}),
        )
        .expect("a schedule");
        let second = create_schedule(
            &graph,
            &scope(),
            &json!({"template_id": template_id, "name": "Second", "today": "2026-03-01T09:05:00Z"}),
        )
        .expect("a schedule");
        for day in 1..=3 {
            run_report(
                &graph,
                &scope(),
                field(&first, "id"),
                &json!({"today": format!("2026-03-0{day}T09:00:00Z")}),
            )
            .expect("a report");
        }
        run_report(
            &graph,
            &scope(),
            field(&second, "id"),
            &json!({"today": "2026-03-09T09:00:00Z"}),
        )
        .expect("a report");

        let all = list_generated_reports(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(all["reports"].as_array().expect("reports").len(), 4);
        let newest_two =
            list_generated_reports(&graph, &scope(), &json!({"limit": "2"})).expect("a listing");
        assert_eq!(newest_two["reports"].as_array().expect("reports").len(), 2);
        assert_eq!(
            newest_two["reports"][1]["schedule_id"],
            json!(field(&second, "id"))
        );
        let only_first = list_generated_reports(
            &graph,
            &scope(),
            &json!({"schedule_id": field(&first, "id")}),
        )
        .expect("a listing");
        assert_eq!(only_first["reports"].as_array().expect("reports").len(), 3);
    }

    #[test]
    fn another_books_template_is_not_mine_to_schedule() {
        let graph = MemoryGraph::new();
        let theirs = BookScope::new("user-2", "book-2");
        let template_id = field(&template(&graph, &theirs), "id").to_string();
        let refused_schedule = create_schedule(
            &graph,
            &scope(),
            &json!({"template_id": template_id, "name": "Borrowed", "today": "2026-03-01T09:00:00Z"}),
        );
        assert!(refused_schedule.is_err());
        // and their listing is not mine
        assert_eq!(
            list_templates(&graph, &scope(), &json!({})).unwrap()["templates"],
            json!([])
        );
    }
}
