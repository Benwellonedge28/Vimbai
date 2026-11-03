//! Execution records: one per attempted run.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{TaskStatus, TaskType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub task_id: String,
    pub task_name: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_seconds: Option<f64>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub retry_attempt: u32,
    pub triggered_by: String,
}

impl TaskExecution {
    /// A record for a run that is about to start (the scheduler path).
    pub fn starting(
        id: impl Into<String>,
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        task_type: TaskType,
        started_at: impl Into<String>,
        triggered_by: impl Into<String>,
    ) -> Self {
        TaskExecution {
            id: id.into(),
            task_id: task_id.into(),
            task_name: task_name.into(),
            task_type,
            status: TaskStatus::Running,
            started_at: Some(started_at.into()),
            completed_at: None,
            duration_seconds: None,
            result: None,
            error: None,
            retry_attempt: 0,
            triggered_by: triggered_by.into(),
        }
    }

    /// The stub the manual trigger left behind: pending, never started, never
    /// updated. Kept because the service really did write one.
    pub fn pending_stub(
        id: impl Into<String>,
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        task_type: TaskType,
        triggered_by: impl Into<String>,
    ) -> Self {
        TaskExecution {
            id: id.into(),
            task_id: task_id.into(),
            task_name: task_name.into(),
            task_type,
            status: TaskStatus::Pending,
            started_at: None,
            completed_at: None,
            duration_seconds: None,
            result: None,
            error: None,
            retry_attempt: 0,
            triggered_by: triggered_by.into(),
        }
    }

    /// Close a running record. Duration comes from the timestamps the caller
    /// supplied, so the same inputs always produce the same numbers.
    pub fn complete(
        &mut self,
        completed_at: impl Into<String>,
        duration_seconds: f64,
        result: Value,
    ) {
        self.status = TaskStatus::Completed;
        self.completed_at = Some(completed_at.into());
        self.duration_seconds = Some(duration_seconds);
        self.result = Some(result);
    }

    pub fn fail(
        &mut self,
        completed_at: impl Into<String>,
        duration_seconds: f64,
        error: impl Into<String>,
    ) {
        self.status = TaskStatus::Failed;
        self.completed_at = Some(completed_at.into());
        self.duration_seconds = Some(duration_seconds);
        self.error = Some(error.into());
    }

    pub fn cancel(&mut self, completed_at: impl Into<String>) {
        self.status = TaskStatus::Cancelled;
        self.completed_at = Some(completed_at.into());
    }

    /// Sort key: records that never started sink below started ones, exactly
    /// like the service's `started_at or datetime.min`.
    pub fn sort_key(&self) -> &str {
        self.started_at.as_deref().unwrap_or("")
    }
}
