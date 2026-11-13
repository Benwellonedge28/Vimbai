#![forbid(unsafe_code)]
//! reporting-service: the reports a Book asks for, and the schedules those reports run on.
//!
//! This service is a shape and nothing more - it generates no document and stores no report. Ported as it shipped,
//! with the same honesty about what it is:
//!
//! * a generated report answers with an id built from the moment it was asked for, the type it was asked for, the
//!   word `generated`, and the recipients it was told about. Nothing is written and no document is produced;
//! * the file url is the one the shipped service returned, straight from `reports.example.com`. It is kept because
//!   a caller may be reading the shape rather than the file, but it says plainly that it is a placeholder rather
//!   than pretending to be a report;
//! * a schedule answers active with its next run at the moment it was created. Nothing is scheduled and nothing
//!   will run; a caller that wants a report to repeat needs a real scheduler, not this service.
//!
//! Anyone reading this expecting generated reports should stop here: the arithmetic lives in the reporting
//! calculations crate, and the documents belong to the service that renders them.

use serde_json::{json, Value};

use vimbai_store::read::refused;
use vimbai_store::StoreError;

/// The address the shipped service put in its answers.
const PLACEHOLDER_HOST: &str = "https://reports.example.com";

/// A moment as the id stamp the shipped service used: `YYYYMMDDHHMM`.
fn stamp(moment: &str) -> String {
    let digits: String = moment
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    digits.chars().take(12).collect()
}

/// The moments this service needs. A request that states neither refuses rather than inventing a clock reading.
fn moment_of(payload: &Value, key: &str) -> Result<String, StoreError> {
    match payload.get(key).and_then(Value::as_str) {
        Some(stated) if !stated.trim().is_empty() => Ok(stated.trim().to_string()),
        _ => Err(refused(&format!("{key} is required"))),
    }
}

fn required(payload: &Value, key: &str) -> Result<String, StoreError> {
    match payload.get(key).and_then(Value::as_str) {
        Some(stated) if !stated.trim().is_empty() => Ok(stated.trim().to_string()),
        _ => Err(refused(&format!("{key} is required"))),
    }
}

fn recipients_of(payload: &Value) -> Vec<String> {
    payload
        .get("recipients")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `POST /generate`: a report asked for. The answer describes the ask, not a document.
pub fn generate_report(payload: &Value) -> Result<Value, StoreError> {
    let company_id = required(payload, "company_id")?;
    let report_type = required(payload, "report_type")?;
    // the period is part of the ask, so a report covering nothing is refused rather than silently unbounded
    for key in ["period_start", "period_end"] {
        required(payload, key)?;
    }
    let asked_at = moment_of(payload, "today")?;
    let recipients = recipients_of(payload);
    Ok(json!({
        "report_id": format!("RPT-{}", stamp(&asked_at)),
        "report_type": report_type,
        "status": "generated",
        "generated_at": asked_at,
        // kept as shipped: the shipped service answered with this address and produced no file
        "file_url": format!("{PLACEHOLDER_HOST}/{company_id}/{report_type}.pdf"),
        "recipients_notified": recipients,
        "placeholder": true,
    }))
}

/// `POST /schedule`: a report asked to repeat. Nothing is scheduled by answering.
pub fn schedule_report(payload: &Value) -> Result<Value, StoreError> {
    required(payload, "company_id")?;
    let report_type = required(payload, "report_type")?;
    let schedule = required(payload, "schedule")?;
    let asked_at = moment_of(payload, "today")?;
    recipients_of(payload);
    Ok(json!({
        "schedule_id": format!("SCH-{}", stamp(&asked_at)),
        "report_type": report_type,
        "schedule": schedule,
        // as shipped: the next run is the moment the schedule was recorded, because nothing runs it
        "next_run": asked_at,
        "active": true,
        "placeholder": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask() -> Value {
        json!({
            "company_id": "co-1",
            "report_type": "trial_balance",
            "period_start": "2026-03-01",
            "period_end": "2026-03-31",
            "format": "pdf",
            "recipients": ["finance@example.com"],
            "today": "2026-03-31T14:05:00+00:00"
        })
    }

    #[test]
    fn a_generated_report_says_what_was_asked_for_and_no_more() {
        let answer = generate_report(&ask()).expect("an answer");
        assert_eq!(answer["report_id"], json!("RPT-202603311405"));
        assert_eq!(answer["status"], json!("generated"));
        assert_eq!(answer["generated_at"], json!("2026-03-31T14:05:00+00:00"));
        assert_eq!(
            answer["recipients_notified"][0],
            json!("finance@example.com")
        );
        // the file is not a file
        assert_eq!(answer["placeholder"], json!(true));
    }

    #[test]
    fn the_file_address_is_the_placeholder_the_service_always_returned() {
        let answer = generate_report(&ask()).expect("an answer");
        assert_eq!(
            answer["file_url"],
            json!("https://reports.example.com/co-1/trial_balance.pdf")
        );
    }

    #[test]
    fn a_report_over_no_period_is_refused() {
        let mut payload = ask();
        payload.as_object_mut().unwrap().remove("period_end");
        assert!(generate_report(&payload).is_err());
    }

    #[test]
    fn a_schedule_answers_active_with_the_moment_it_was_recorded() {
        let answer = schedule_report(&json!({
            "company_id": "co-1",
            "report_type": "cash_flow",
            "schedule": "monthly",
            "recipients": [],
            "parameters": {"basis": "accrual"},
            "today": "2026-03-31T14:05:00+00:00"
        }))
        .expect("an answer");
        assert_eq!(answer["schedule_id"], json!("SCH-202603311405"));
        assert_eq!(answer["schedule"], json!("monthly"));
        assert_eq!(answer["next_run"], json!("2026-03-31T14:05:00+00:00"));
        assert_eq!(answer["active"], json!(true));
    }

    #[test]
    fn a_moment_is_read_from_the_request_rather_than_a_clock() {
        // the same ask at two stated moments gives two ids, and asking twice at one moment gives one
        let mut later = ask();
        later["today"] = json!("2026-04-01T09:00:00+00:00");
        let first = generate_report(&ask()).expect("an answer");
        let repeated = generate_report(&ask()).expect("an answer");
        let after = generate_report(&later).expect("an answer");
        assert_eq!(first["report_id"], repeated["report_id"]);
        assert_ne!(first["report_id"], after["report_id"]);
    }
}
