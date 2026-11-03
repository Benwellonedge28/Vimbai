//! The scheduler state machine, caller-owned.
//!
//! The Python service kept `tasks`, `executions` and `running_tasks` as module
//! globals shared by every caller. Here the caller holds the `Engine`, so two
//! books can schedule independently and nothing leaks between them.

use indexmap::IndexMap;
use serde_json::{json, Value};

use crate::defaults::default_tasks;
use crate::execution::TaskExecution;
use crate::task::{TaskDefinition, TaskRequest};
use crate::types::{TaskStatus, TaskType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The service answered 404 "Task not found".
    TaskNotFound,
    /// The service answered 404 "Execution not found or already completed".
    ExecutionNotFound,
    /// The service answered 422 for an unknown task type.
    UnknownTaskType(String),
}

impl EngineError {
    /// The status code the HTTP layer should use.
    pub fn status_code(&self) -> u16 {
        match self {
            EngineError::UnknownTaskType(_) => 422,
            _ => 404,
        }
    }

    pub fn message(&self) -> String {
        match self {
            EngineError::TaskNotFound => "Task not found".to_string(),
            EngineError::ExecutionNotFound => {
                "Execution not found or already completed".to_string()
            }
            EngineError::UnknownTaskType(name) => format!("Unknown task type: {name}"),
        }
    }
}

/// What a manual trigger produced. The service wrote two records for one run:
/// a pending stub from the trigger, then a scheduler-tagged running record from
/// the worker. Both are returned here rather than hidden, because clients see
/// both in the history.
#[derive(Debug, Clone, PartialEq)]
pub struct Trigger {
    pub pending: TaskExecution,
    pub started: TaskExecution,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    pub total_tasks: usize,
    pub enabled_tasks: usize,
    pub running_tasks: usize,
    pub total_executions: usize,
    pub completed_executions: usize,
    pub failed_executions: usize,
    pub success_rate: f64,
}

impl Metrics {
    pub fn to_json(&self) -> Value {
        json!({
            "total_tasks": self.total_tasks,
            "enabled_tasks": self.enabled_tasks,
            "running_tasks": self.running_tasks,
            "total_executions": self.total_executions,
            "completed_executions": self.completed_executions,
            "failed_executions": self.failed_executions,
            "success_rate": self.success_rate,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Engine {
    tasks: IndexMap<String, TaskDefinition>,
    executions: IndexMap<String, Vec<TaskExecution>>,
    running: IndexMap<String, TaskExecution>,
}

impl Engine {
    pub fn new() -> Self {
        Engine::default()
    }

    /// A fresh engine carrying the seven tasks the service shipped with.
    pub fn with_defaults() -> Self {
        let mut engine = Engine::new();
        for task in default_tasks() {
            engine.tasks.insert(task.id.clone(), task);
        }
        engine
    }

    pub fn tasks(&self) -> Vec<TaskDefinition> {
        self.tasks.values().cloned().collect()
    }

    pub fn list_tasks(&self, enabled_only: bool) -> Vec<TaskDefinition> {
        self.tasks
            .values()
            .filter(|t| !enabled_only || t.enabled)
            .cloned()
            .collect()
    }

    pub fn get_task(&self, task_id: &str) -> Result<TaskDefinition, EngineError> {
        self.tasks
            .get(task_id)
            .cloned()
            .ok_or(EngineError::TaskNotFound)
    }

    pub fn create_task(&mut self, id: impl Into<String>, request: &TaskRequest) -> TaskDefinition {
        let task = TaskDefinition::from_request(id, request);
        self.tasks.insert(task.id.clone(), task.clone());
        task
    }

    /// Create from a raw wire body, so the strict task-type contract is enforced
    /// in one place.
    pub fn create_from_value(
        &mut self,
        id: impl Into<String>,
        body: &Value,
    ) -> Result<TaskDefinition, EngineError> {
        let request = TaskRequest::from_value(body).map_err(|_| {
            EngineError::UnknownTaskType(
                body.get("task_type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        })?;
        Ok(self.create_task(id, &request))
    }

    pub fn update_task(
        &mut self,
        task_id: &str,
        request: &TaskRequest,
    ) -> Result<TaskDefinition, EngineError> {
        if !self.tasks.contains_key(task_id) {
            return Err(EngineError::TaskNotFound);
        }
        // The service replaced the request-derived fields and kept the id and the
        // knobs that were never exposed (retry delay, concurrency).
        let mut task = TaskDefinition::from_request(task_id, request);
        if let Some(existing) = self.tasks.get(task_id) {
            task.retry_delay_seconds = existing.retry_delay_seconds;
            task.notification_on_failure = existing.notification_on_failure;
            task.run_immediately = existing.run_immediately;
            task.max_concurrent = existing.max_concurrent;
        }
        self.tasks.insert(task_id.to_string(), task.clone());
        Ok(task)
    }

    pub fn delete_task(&mut self, task_id: &str) -> Result<(), EngineError> {
        if self.tasks.shift_remove(task_id).is_some() {
            // The service left the execution history behind when a task was
            // deleted; keep that, so history stays readable.
            Ok(())
        } else {
            Err(EngineError::TaskNotFound)
        }
    }

    pub fn set_enabled(&mut self, task_id: &str, enabled: bool) -> Result<(), EngineError> {
        match self.tasks.get_mut(task_id) {
            Some(task) => {
                task.enabled = enabled;
                Ok(())
            }
            None => Err(EngineError::TaskNotFound),
        }
    }

    /// The manual trigger: writes the pending stub the trigger left behind and
    /// the running record the worker started.
    pub fn trigger(
        &mut self,
        task_id: &str,
        triggered_by: &str,
        pending_id: impl Into<String>,
        started_id: impl Into<String>,
        started_at: impl Into<String>,
    ) -> Result<Trigger, EngineError> {
        let task = self
            .tasks
            .get(task_id)
            .cloned()
            .ok_or(EngineError::TaskNotFound)?;
        let pending = TaskExecution::pending_stub(
            pending_id,
            task.id.clone(),
            task.name.clone(),
            task.task_type,
            triggered_by,
        );
        let started = TaskExecution::starting(
            started_id,
            task.id.clone(),
            task.name.clone(),
            task.task_type,
            started_at,
            "scheduler",
        );
        self.executions
            .entry(task.id.clone())
            .or_default()
            .extend([pending.clone(), started.clone()]);
        self.running.insert(task.id.clone(), started.clone());
        Ok(Trigger { pending, started })
    }

    /// Close a running execution with success.
    pub fn complete_execution(
        &mut self,
        execution_id: &str,
        completed_at: impl Into<String>,
        duration_seconds: f64,
        result: Value,
    ) -> Result<TaskExecution, EngineError> {
        let (task_id, mut execution) = self.take_running(execution_id)?;
        execution.complete(completed_at, duration_seconds, result);
        self.replace_record(&task_id, execution.clone());
        Ok(execution)
    }

    /// Close a running execution with a failure.
    pub fn fail_execution(
        &mut self,
        execution_id: &str,
        completed_at: impl Into<String>,
        duration_seconds: f64,
        error: impl Into<String>,
    ) -> Result<TaskExecution, EngineError> {
        let (task_id, mut execution) = self.take_running(execution_id)?;
        execution.fail(completed_at, duration_seconds, error);
        self.replace_record(&task_id, execution.clone());
        Ok(execution)
    }

    pub fn cancel_execution(&mut self, execution_id: &str) -> Result<TaskExecution, EngineError> {
        let (task_id, mut execution) = self.take_running(execution_id)?;
        execution.cancel("");
        // The service stamped completed_at with "now"; the caller supplies the
        // value through cancel_execution_at when it matters.
        let _ = &task_id;
        self.replace_record(&task_id, execution.clone());
        Ok(execution)
    }

    /// Cancel with an explicit timestamp.
    pub fn cancel_execution_at(
        &mut self,
        execution_id: &str,
        completed_at: impl Into<String>,
    ) -> Result<TaskExecution, EngineError> {
        let (task_id, mut execution) = self.take_running(execution_id)?;
        execution.cancel(completed_at);
        self.replace_record(&task_id, execution.clone());
        Ok(execution)
    }

    /// Cancel by task id, the scheduler's view of "stop this task".
    pub fn cancel_task(&mut self, task_id: &str, completed_at: impl Into<String>) -> bool {
        match self.running.shift_remove(task_id) {
            Some(mut execution) => {
                execution.cancel(completed_at);
                self.replace_record(task_id, execution);
                true
            }
            None => false,
        }
    }

    pub fn running(&self) -> Vec<TaskExecution> {
        self.running.values().cloned().collect()
    }

    /// History for one task, newest first, optionally filtered by status.
    /// Pending records never started, so they sort below started ones.
    pub fn executions_for(
        &self,
        task_id: &str,
        status: Option<TaskStatus>,
        limit: usize,
    ) -> Vec<TaskExecution> {
        let Some(records) = self.executions.get(task_id) else {
            return Vec::new();
        };
        let mut selected: Vec<TaskExecution> = records
            .iter()
            .filter(|e| status.is_none_or(|want| e.status == want))
            .cloned()
            .collect();
        selected.sort_by(|a, b| b.sort_key().cmp(a.sort_key()));
        selected.truncate(limit);
        selected
    }

    pub fn task_types() -> Vec<Value> {
        TaskType::catalogue()
    }

    pub fn metrics(&self) -> Metrics {
        let total_executions: usize = self.executions.values().map(Vec::len).sum();
        let completed = self
            .executions
            .values()
            .flatten()
            .filter(|e| e.status == TaskStatus::Completed)
            .count();
        let failed = self
            .executions
            .values()
            .flatten()
            .filter(|e| e.status == TaskStatus::Failed)
            .count();
        Metrics {
            total_tasks: self.tasks.len(),
            enabled_tasks: self.tasks.values().filter(|t| t.enabled).count(),
            running_tasks: self.running.len(),
            total_executions,
            completed_executions: completed,
            failed_executions: failed,
            success_rate: if total_executions == 0 {
                0.0
            } else {
                completed as f64 / total_executions as f64
            },
        }
    }

    fn take_running(&mut self, execution_id: &str) -> Result<(String, TaskExecution), EngineError> {
        let task_id = self
            .running
            .iter()
            .find(|(_, e)| e.id == execution_id)
            .map(|(task_id, _)| task_id.clone())
            .ok_or(EngineError::ExecutionNotFound)?;
        let execution = self
            .running
            .shift_remove(&task_id)
            .ok_or(EngineError::ExecutionNotFound)?;
        Ok((task_id, execution))
    }

    fn replace_record(&mut self, task_id: &str, updated: TaskExecution) {
        if let Some(records) = self.executions.get_mut(task_id) {
            if let Some(slot) = records.iter_mut().find(|e| e.id == updated.id) {
                *slot = updated;
            }
        }
    }
}
