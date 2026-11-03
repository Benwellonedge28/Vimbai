//! Task definitions and the request that creates or replaces one.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::types::TaskType;

/// A scheduled unit of work. Mirrors the service's TaskDefinition, including
/// the knobs that were never read (run_immediately, max_concurrent) so the wire
/// shape stays identical.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub task_type: TaskType,
    pub schedule: String,
    pub enabled: bool,
    pub service_endpoint: String,
    pub payload: Value,
    pub timeout_seconds: u64,
    pub retry_count: u32,
    pub retry_delay_seconds: u64,
    pub notification_on_failure: bool,
    pub run_immediately: bool,
    pub max_concurrent: u32,
}

impl TaskDefinition {
    /// Build from a create/update request. The service accepted a subset of
    /// fields on the wire and filled the rest with defaults; so does this.
    pub fn from_request(id: impl Into<String>, request: &TaskRequest) -> Self {
        TaskDefinition {
            id: id.into(),
            name: request.name.clone(),
            description: request.description.clone(),
            task_type: request.task_type,
            schedule: request.schedule.clone(),
            enabled: request.enabled,
            service_endpoint: request.service_endpoint.clone(),
            payload: request.payload.clone(),
            timeout_seconds: request.timeout_seconds,
            retry_count: request.retry_count,
            retry_delay_seconds: 60,
            notification_on_failure: true,
            run_immediately: false,
            max_concurrent: 1,
        }
    }
}

/// The create/update body. Unknown task types are rejected by `TaskType::parse`
/// before this is built.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub task_type: TaskType,
    pub schedule: String,
    pub service_endpoint: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_retries")]
    pub retry_count: u32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_timeout() -> u64 {
    300
}

fn default_retries() -> u32 {
    3
}

fn default_enabled() -> bool {
    true
}

impl TaskRequest {
    /// Parse a wire body, keeping the service's "strict task type" contract.
    pub fn from_value(value: &Value) -> Result<TaskRequest, String> {
        let map: &Map<String, Value> = value
            .as_object()
            .ok_or_else(|| "Request body must be an object".to_string())?;
        let task_type_value = map
            .get("task_type")
            .and_then(Value::as_str)
            .ok_or_else(|| "task_type is required".to_string())?;
        let task_type = TaskType::parse(task_type_value)?;
        Ok(TaskRequest {
            name: map
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description: map
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            task_type,
            schedule: map
                .get("schedule")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            service_endpoint: map
                .get("service_endpoint")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            payload: map
                .get("payload")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new())),
            timeout_seconds: map
                .get("timeout_seconds")
                .and_then(Value::as_u64)
                .unwrap_or_else(default_timeout),
            retry_count: map
                .get("retry_count")
                .and_then(Value::as_u64)
                .map(|v| v as u32)
                .unwrap_or_else(default_retries),
            enabled: map
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or_else(default_enabled),
        })
    }
}
