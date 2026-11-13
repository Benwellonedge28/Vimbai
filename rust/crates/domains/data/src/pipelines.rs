#![forbid(unsafe_code)]
//! etl-service: the pipelines a Book defines, the steps they walk, and the runs that walked them.
//!
//! A pipeline is a durable DEFINITION, not a running process, and the port keeps that distinction:
//!
//! * a pipeline is stored when it is described, with its steps in the order given and a status of pending. Describing it
//!   does not run it;
//! * running one walks every step and stamps each as completed, then the pipeline as completed, carrying the moment it
//!   ran. A step reports zero rows, because the shipped service simulated the work - and a count of zero is the honest
//!   record of work that never touched a row;
//! * a pipeline with no steps still runs: it completes, with nothing walked, and its last run is still stamped. Refusing
//!   an empty pipeline would refuse a definition that is merely not finished yet;
//! * a status filter narrows the listing to that status only, and a pipeline bookmarked by id answers whether or not it
//!   matches a filter elsewhere - the filter belongs to the listing, not to the record;
//! * deleting is by id, answers what it deleted, and a pipeline that is not there is missing rather than silently gone.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A pipeline definition: a name, its steps, and the schedule it is meant to run on.
pub const PIPELINE: RecordKind = RecordKind::new("EtlPipeline", "OWNS_ETL_PIPELINE");

fn steps_of(payload: &Value) -> Vec<Value> {
    match payload.get("steps") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|step| {
                let mut step = step.clone();
                if let Some(object) = step.as_object_mut() {
                    if !object.contains_key("id") || object["id"].is_null() {
                        object.insert("id".to_string(), json!(service_id()));
                    }
                    for field in [
                        "source",
                        "target",
                        "status",
                        "error_message",
                        "started_at",
                        "completed_at",
                    ] {
                        object.entry(field).or_insert(Value::Null);
                    }
                    object.insert("rows_processed".to_string(), json!(0));
                    object
                        .entry("config".to_string())
                        .or_insert_with(|| json!({}));
                }
                step
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `POST /pipelines`: describe a pipeline. Describing does not run it.
pub fn create_pipeline(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.trim().is_empty() {
        return Err(refused("name is required"));
    }
    Records::new(graph, PIPELINE).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "name": name,
            "description": text(payload, "description"),
            "steps": steps_of(payload),
            "status": "pending",
            "schedule": text(payload, "schedule"),
            "created_at": payload.get("now").cloned().unwrap_or(Value::Null),
            "last_run": Value::Null,
        }),
    )
}

/// `GET /pipelines`: the Book's pipelines, narrowed to one status when asked.
pub fn pipelines(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "status");
    let rows = Records::new(graph, PIPELINE).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| wanted.is_empty() || text(row, "status") == wanted)
            .collect(),
    ))
}

/// `GET /pipelines/{id}`: one pipeline.
pub fn pipeline(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    pipeline_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, PIPELINE).get(scope, pipeline_id)
}

/// `POST /pipelines/{id}/run`: walk every step and record the run.
pub fn run_pipeline(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    pipeline_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = pipeline(graph, scope, pipeline_id)?;
    // the caller may state the moment (so a reckoning repeats), and the run is stamped from the clock when they do not
    let now = match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    };
    // every step is walked in the order it was described, and each one reports what it touched - nothing
    let mut steps = match held.get("steps") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    for step in steps.iter_mut() {
        if let Some(object) = step.as_object_mut() {
            object.insert("status".to_string(), json!("completed"));
            object.insert("rows_processed".to_string(), json!(0));
            object.insert("started_at".to_string(), now.clone());
            object.insert("completed_at".to_string(), now.clone());
        }
    }
    let walked = steps.len();
    Records::new(graph, PIPELINE).update(
        scope,
        pipeline_id,
        &json!({
            "status": "completed",
            "last_run": now,
            "steps": steps,
        }),
    )?;
    Ok(json!({
        "pipeline_id": pipeline_id,
        "status": "completed",
        "steps_completed": walked,
        "run_at": now,
    }))
}

/// `DELETE /pipelines/{id}`: forget a pipeline.
pub fn delete_pipeline(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    pipeline_id: &str,
) -> Result<Value, StoreError> {
    pipeline(graph, scope, pipeline_id)?;
    Records::new(graph, PIPELINE).delete(scope, pipeline_id)?;
    Ok(json!({"deleted": true, "pipeline_id": pipeline_id}))
}

/// How many steps a stored pipeline carries, in the order it will walk them.
pub fn steps_held(pipeline_row: &Value) -> u64 {
    pipeline_row
        .get("steps")
        .and_then(Value::as_array)
        .map(|steps| steps.len() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn described(graph: &MemoryGraph) -> Value {
        create_pipeline(
            graph,
            &talked(),
            &json!({"name": "sales to warehouse", "description": "nightly",
                    "steps": [{"name": "extract", "step_type": "extract", "source": "raw_sales"},
                              {"name": "load", "step_type": "load", "target": "sales"}],
                    "schedule": "0 2 * * *", "now": 1000.0}),
        )
        .expect("a pipeline")
    }

    #[test]
    fn describing_a_pipeline_does_not_run_it() {
        let graph = MemoryGraph::new();
        let made = described(&graph);
        assert_eq!(made["status"], json!("pending"));
        assert_eq!(made["last_run"], Value::Null);
        assert_eq!(made["steps"].as_array().map(Vec::len), Some(2));
        // a step keeps what was said about it and starts with nothing processed
        assert_eq!(made["steps"][0]["step_type"], json!("extract"));
        assert_eq!(made["steps"][0]["rows_processed"], json!(0));
        assert!(made["steps"][0]["id"].as_str().unwrap_or("").len() > 4);
        // naming nothing is refused
        assert!(create_pipeline(&graph, &talked(), &json!({"name": " "})).is_err());
    }

    #[test]
    fn running_walks_every_step_and_stamps_the_run() {
        let graph = MemoryGraph::new();
        let made = described(&graph);
        let id = text(&made, "id");
        let ran = run_pipeline(&graph, &talked(), &id, &json!({"now": 2000.0})).expect("a run");
        assert_eq!(ran["status"], json!("completed"));
        assert_eq!(ran["steps_completed"], json!(2));
        assert_eq!(ran["run_at"], json!(2000.0));
        let after = pipeline(&graph, &talked(), &id).expect("the pipeline");
        assert_eq!(after["status"], json!("completed"));
        assert_eq!(after["last_run"], json!(2000.0));
        assert_eq!(after["steps"][1]["status"], json!("completed"));
        assert_eq!(steps_held(&after), 2);
        // a pipeline that is not there is missing, including another Book's
        assert!(run_pipeline(&graph, &talked(), "nope", &json!({})).is_err());
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(run_pipeline(&graph, &elsewhere, &id, &json!({})).is_err());
    }

    #[test]
    fn an_empty_pipeline_still_runs_and_the_filter_narrows_the_listing() {
        let graph = MemoryGraph::new();
        let empty = create_pipeline(&graph, &talked(), &json!({"name": "placeholder"}))
            .expect("a pipeline");
        let id = text(&empty, "id");
        let ran = run_pipeline(&graph, &talked(), &id, &json!({"now": 3000.0})).expect("a run");
        assert_eq!(ran["steps_completed"], json!(0));
        assert_eq!(ran["status"], json!("completed"));
        let after = pipeline(&graph, &talked(), &id).expect("the pipeline");
        assert_eq!(after["last_run"], json!(3000.0));

        described(&graph);
        // the filter narrows the listing, and each status is answered on its own
        let done =
            pipelines(&graph, &talked(), &json!({"status": "completed"})).expect("pipelines");
        let pending =
            pipelines(&graph, &talked(), &json!({"status": "pending"})).expect("pipelines");
        assert_eq!(done.as_array().map(Vec::len), Some(1));
        assert_eq!(pending.as_array().map(Vec::len), Some(1));
        assert_eq!(
            pipelines(&graph, &talked(), &json!({}))
                .expect("pipelines")
                .as_array()
                .map(Vec::len),
            Some(2)
        );

        // deleting answers what went, and deleting again finds nothing
        let gone = delete_pipeline(&graph, &talked(), &id).expect("a delete");
        assert_eq!(gone["deleted"], json!(true));
        assert!(delete_pipeline(&graph, &talked(), &id).is_err());
        assert_eq!(
            pipelines(&graph, &talked(), &json!({}))
                .expect("pipelines")
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }
}
