"""
Vimbai Automation Engine
Scheduler and worker system for executing autonomous tasks across microservices
"""

import asyncio
import os
import uuid
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Callable, Dict, List, Optional

import croniter
from dotenv import load_dotenv
from fastapi import BackgroundTasks, Depends, FastAPI, HTTPException
from pydantic import BaseModel, Field

load_dotenv()

app = FastAPI(
    title="Vimbai Automation Engine",
    description="Scheduler and worker for autonomous financial tasks",
    version="1.0.0",
)

# ============================================================================
# Models
# ============================================================================


class TaskStatus(str, Enum):
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"
    SKIPPED = "skipped"


class TaskType(str, Enum):
    # Accounting Tasks
    RECONCILE_ACCOUNTS = "reconcile_accounts"
    GENERATE_TRIAL_BALANCE = "generate_trial_balance"
    POST_JOURNAL_ENTRIES = "post_journal_entries"
    DEPRECIATION_CALCULATION = "depreciation_calculation"
    YEAR_END_CLOSE = "year_end_close"
    MONTH_END_CLOSE = "month_end_close"

    # Finance Tasks
    BUDGET_VARIANCE_CHECK = "budget_variance_check"
    CASHFLOW_FORECAST_UPDATE = "cashflow_forecast_update"
    FINANCIAL_RATIO_CALCULATION = "financial_ratio_calculation"
    SCENARIO_REFRESH = "scenario_refresh"

    # Integration Tasks
    SYNC_POS_DATA = "sync_pos_data"
    BANK_FEED_PROCESSING = "bank_feed_processing"
    INVENTORY_RECONCILIATION = "inventory_reconciliation"
    CRM_SYNC = "crm_sync"

    # Fraud & Compliance
    FRAUD_SCAN = "fraud_scan"
    COMPLIANCE_CHECK = "compliance_check"
    AUDIT_TRAIL_EXPORT = "audit_trail_export"

    # Reporting
    GENERATE_FINANCIAL_STATEMENTS = "generate_financial_statements"
    DASHBOARD_REFRESH = "dashboard_refresh"
    SCHEDULED_REPORT_GENERATION = "scheduled_report_generation"

    # Multimodal
    DOCUMENT_PROCESSING_BATCH = "document_processing_batch"
    OCR_QUEUE_PROCESSING = "ocr_queue_processing"

    # System
    DATA_BACKUP = "data_backup"
    CACHE_CLEAR = "cache_clear"
    SYNC_OFFLINE_DATA = "sync_offline_data"


class TaskDefinition(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: Optional[str] = None
    task_type: TaskType
    schedule: str  # Cron expression
    enabled: bool = True
    service_endpoint: str  # Service to call
    payload: Dict[str, Any] = {}
    timeout_seconds: int = 300
    retry_count: int = 3
    retry_delay_seconds: int = 60
    notification_on_failure: bool = True
    run_immediately: bool = False
    max_concurrent: int = 1


class TaskExecution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    task_id: str
    task_name: str
    task_type: TaskType
    status: TaskStatus
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    duration_seconds: Optional[float] = None
    result: Optional[Dict[str, Any]] = None
    error: Optional[str] = None
    retry_attempt: int = 0
    triggered_by: str = "scheduler"  # scheduler, manual, event


class TaskCreateRequest(BaseModel):
    name: str
    description: Optional[str] = None
    task_type: TaskType
    schedule: str
    service_endpoint: str
    payload: Dict[str, Any] = {}
    timeout_seconds: int = 300
    retry_count: int = 3
    enabled: bool = True


# ============================================================================
# Task Store
# ============================================================================

tasks: Dict[str, TaskDefinition] = {}
executions: Dict[str, List[TaskExecution]] = {}
running_tasks: Dict[str, TaskExecution] = {}

# Default tasks
DEFAULT_TASKS = [
    TaskDefinition(
        id="daily-reconciliation",
        name="Daily Account Reconciliation",
        description="Automated daily bank account reconciliation",
        task_type=TaskType.RECONCILE_ACCOUNTS,
        schedule="0 2 * * *",  # 2 AM daily
        service_endpoint="/api/reconcile",
        enabled=True,
    ),
    TaskDefinition(
        id="trial-balance-gen",
        name="Generate Trial Balance",
        description="Generate daily trial balance report",
        task_type=TaskType.GENERATE_TRIAL_BALANCE,
        schedule="0 1 * * *",  # 1 AM daily
        service_endpoint="/api/trial-balance/generate",
        enabled=True,
    ),
    TaskDefinition(
        id="fraud-scan-daily",
        name="Daily Fraud Scan",
        description="Scan all transactions for fraud patterns",
        task_type=TaskType.FRAUD_SCAN,
        schedule="0 3 * * *",  # 3 AM daily
        service_endpoint="/api/fraud/scan-all",
        enabled=True,
    ),
    TaskDefinition(
        id="budget-variance-check",
        name="Budget Variance Check",
        description="Check budget variances and alert",
        task_type=TaskType.BUDGET_VARIANCE_CHECK,
        schedule="0 9 * * *",  # 9 AM daily
        service_endpoint="/api/budgets/check-variance",
        enabled=True,
    ),
    TaskDefinition(
        id="cashflow-forecast",
        name="Cash Flow Forecast Update",
        description="Update cash flow forecast projections",
        task_type=TaskType.CASHFLOW_FORECAST_UPDATE,
        schedule="0 6 * * *",  # 6 AM daily
        service_endpoint="/api/finance/update-forecast",
        enabled=True,
    ),
    TaskDefinition(
        id="dashboard-refresh",
        name="Dashboard Refresh",
        description="Refresh all dashboard data",
        task_type=TaskType.DASHBOARD_REFRESH,
        schedule="*/15 * * * *",  # Every 15 minutes
        service_endpoint="/api/dashboards/refresh",
        timeout_seconds=120,
        enabled=True,
    ),
    TaskDefinition(
        id="month-end-close",
        name="Month-End Close",
        description="Execute month-end closing procedures",
        task_type=TaskType.MONTH_END_CLOSE,
        schedule="0 0 28-31 * *",  # Last day of month
        service_endpoint="/api/accounting/month-end",
        enabled=True,
    ),
]

for task in DEFAULT_TASKS:
    tasks[task.id] = task


# ============================================================================
# Automation Engine Core
# ============================================================================


class AutomationEngine:
    """Main automation engine for scheduling and executing tasks"""

    def __init__(self):
        self.scheduler_task = None
        self.is_running = False

    async def start(self):
        """Start the automation engine"""
        if self.is_running:
            return

        self.is_running = True
        self.scheduler_task = asyncio.create_task(self._scheduler_loop())
        print("[AutomationEngine] Started")

    async def stop(self):
        """Stop the automation engine"""
        self.is_running = False
        if self.scheduler_task:
            self.scheduler_task.cancel()
            try:
                await self.scheduler_task
            except asyncio.CancelledError:
                pass
        print("[AutomationEngine] Stopped")

    async def _scheduler_loop(self):
        """Main scheduler loop - checks every minute for tasks to run"""
        while self.is_running:
            try:
                now = datetime.utcnow()

                for task_id, task in tasks.items():
                    if not task.enabled:
                        continue

                    # Check if task should run now
                    if self._should_run(task, now):
                        # Check if task is already running
                        if task_id not in running_tasks:
                            asyncio.create_task(self._execute_task(task))
                        else:
                            print(f"[AutomationEngine] Task {task.name} already running, skipping")

                await asyncio.sleep(60)  # Check every minute

            except asyncio.CancelledError:
                break
            except Exception as e:
                print(f"[AutomationEngine] Scheduler error: {e}")
                await asyncio.sleep(60)

    def _should_run(self, task: TaskDefinition, now: datetime) -> bool:
        """Check if task should run at the given time"""
        try:
            cron = croniter.croniter(task.schedule, now)
            prev_run = cron.get_prev(datetime)
            next_run = cron.get_next(datetime)

            # Check if next run is within the next minute
            if next_run - now <= timedelta(minutes=1):
                # Check if we just ran it (within last 2 minutes)
                if prev_run and (now - prev_run) < timedelta(minutes=2):
                    return False
                return True

        except Exception as e:
            print(f"[AutomationEngine] Cron parsing error for {task.name}: {e}")

        return False

    async def _execute_task(self, task: TaskDefinition):
        """Execute a task"""
        task_id = task.id

        # Create execution record
        execution = TaskExecution(
            id=str(uuid.uuid4()),
            task_id=task.id,
            task_name=task.name,
            task_type=task.task_type,
            status=TaskStatus.RUNNING,
            started_at=datetime.utcnow(),
            triggered_by="scheduler",
        )

        running_tasks[task_id] = execution

        if task_id not in executions:
            executions[task_id] = []
        executions[task_id].append(execution)

        print(f"[AutomationEngine] Executing task: {task.name}")

        try:
            # Simulate task execution (in production, this would call the service endpoint)
            await asyncio.sleep(2)  # Simulate work

            # Simulate success
            execution.status = TaskStatus.COMPLETED
            execution.completed_at = datetime.utcnow()
            execution.duration_seconds = (execution.completed_at - execution.started_at).total_seconds()
            execution.result = {"success": True, "message": "Task completed successfully"}

            print(f"[AutomationEngine] Task {task.name} completed successfully")

        except Exception as e:
            execution.status = TaskStatus.FAILED
            execution.completed_at = datetime.utcnow()
            execution.duration_seconds = (execution.completed_at - execution.started_at).total_seconds()
            execution.error = str(e)

            print(f"[AutomationEngine] Task {task.name} failed: {e}")

            # Retry if enabled
            if execution.retry_attempt < task.retry_count:
                execution.retry_attempt += 1
                print(f"[AutomationEngine] Scheduling retry {execution.retry_attempt}/{task.retry_count}")
                await asyncio.sleep(task.retry_delay_seconds)
                asyncio.create_task(self._execute_task(task))

        finally:
            if task_id in running_tasks:
                del running_tasks[task_id]

    async def run_task_now(self, task_id: str, triggered_by: str = "manual") -> TaskExecution:
        """Manually trigger a task to run immediately"""
        if task_id not in tasks:
            raise HTTPException(status_code=404, detail="Task not found")

        task = tasks[task_id]

        execution = TaskExecution(
            id=str(uuid.uuid4()),
            task_id=task.id,
            task_name=task.name,
            task_type=task.task_type,
            status=TaskStatus.PENDING,
            triggered_by=triggered_by,
        )

        if task_id not in executions:
            executions[task_id] = []
        executions[task_id].append(execution)

        # Execute asynchronously
        asyncio.create_task(self._execute_task(task))

        return execution

    async def cancel_task(self, task_id: str) -> bool:
        """Cancel a running task"""
        if task_id in running_tasks:
            execution = running_tasks[task_id]
            execution.status = TaskStatus.CANCELLED
            execution.completed_at = datetime.utcnow()
            del running_tasks[task_id]
            return True
        return False


# Global engine instance
engine = AutomationEngine()


# ============================================================================
# API Endpoints
# ============================================================================


@app.on_event("startup")
async def startup():
    await engine.start()


@app.on_event("shutdown")
async def shutdown():
    await engine.stop()


@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "automation-engine",
        "running": engine.is_running,
        "active_tasks": len(running_tasks),
        "total_tasks": len(tasks),
    }


# --- Task Management ---


@app.post("/tasks", status_code=201)
async def create_task(task_req: TaskCreateRequest):
    """Create a new automated task"""
    task = TaskDefinition(
        name=task_req.name,
        description=task_req.description,
        task_type=task_req.task_type,
        schedule=task_req.schedule,
        service_endpoint=task_req.service_endpoint,
        payload=task_req.payload,
        timeout_seconds=task_req.timeout_seconds,
        retry_count=task_req.retry_count,
        enabled=task_req.enabled,
    )

    tasks[task.id] = task
    return task


@app.get("/tasks")
async def list_tasks(enabled_only: bool = False):
    """List all tasks"""
    result = list(tasks.values())
    if enabled_only:
        result = [t for t in result if t.enabled]
    return result


@app.get("/tasks/{task_id}")
async def get_task(task_id: str):
    """Get a specific task"""
    if task_id not in tasks:
        raise HTTPException(status_code=404, detail="Task not found")
    return tasks[task_id]


@app.put("/tasks/{task_id}")
async def update_task(task_id: str, task_req: TaskCreateRequest):
    """Update a task"""
    if task_id not in tasks:
        raise HTTPException(status_code=404, detail="Task not found")

    task = tasks[task_id]
    task.name = task_req.name
    task.description = task_req.description
    task.task_type = task_req.task_type
    task.schedule = task_req.schedule
    task.service_endpoint = task_req.service_endpoint
    task.payload = task_req.payload
    task.timeout_seconds = task_req.timeout_seconds
    task.retry_count = task_req.retry_count
    task.enabled = task_req.enabled

    return task


@app.delete("/tasks/{task_id}")
async def delete_task(task_id: str):
    """Delete a task"""
    if task_id in tasks:
        del tasks[task_id]
        return {"status": "deleted"}
    raise HTTPException(status_code=404, detail="Task not found")


@app.post("/tasks/{task_id}/enable")
async def enable_task(task_id: str):
    """Enable a task"""
    if task_id not in tasks:
        raise HTTPException(status_code=404, detail="Task not found")
    tasks[task_id].enabled = True
    return {"status": "enabled"}


@app.post("/tasks/{task_id}/disable")
async def disable_task(task_id: str):
    """Disable a task"""
    if task_id not in tasks:
        raise HTTPException(status_code=404, detail="Task not found")
    tasks[task_id].enabled = False
    return {"status": "disabled"}


# --- Task Execution ---


@app.post("/tasks/{task_id}/run")
async def run_task_now(task_id: str):
    """Manually trigger a task to run immediately"""
    execution = await engine.run_task_now(task_id, "manual")
    return {
        "status": "triggered",
        "execution_id": execution.id,
        "task_name": execution.task_name,
    }


@app.get("/tasks/{task_id}/executions")
async def get_task_executions(task_id: str, limit: int = 50, status: Optional[TaskStatus] = None):
    """Get execution history for a task"""
    if task_id not in executions:
        return []

    result = executions[task_id]
    if status:
        result = [e for e in result if e.status == status]

    result.sort(key=lambda x: x.started_at or datetime.min, reverse=True)
    return result[:limit]


@app.get("/executions/running")
async def get_running_executions():
    """Get all currently running executions"""
    return list(running_tasks.values())


@app.post("/executions/{execution_id}/cancel")
async def cancel_execution(execution_id: str):
    """Cancel a running execution"""
    for task_id, execution in running_tasks.items():
        if execution.id == execution_id:
            execution.status = TaskStatus.CANCELLED
            execution.completed_at = datetime.utcnow()
            del running_tasks[task_id]
            return {"status": "cancelled"}

    raise HTTPException(status_code=404, detail="Execution not found or already completed")


# --- Task Types ---


@app.get("/task-types")
async def list_task_types():
    """List all available task types"""
    return [{"name": tt.name, "value": tt.value} for tt in TaskType]


# --- Metrics ---


@app.get("/metrics")
async def get_metrics():
    """Get automation engine metrics"""
    total_executions = sum(len(exec_list) for exec_list in executions.values())
    completed = sum(1 for exec_list in executions.values() for e in exec_list if e.status == TaskStatus.COMPLETED)
    failed = sum(1 for exec_list in executions.values() for e in exec_list if e.status == TaskStatus.FAILED)

    return {
        "total_tasks": len(tasks),
        "enabled_tasks": sum(1 for t in tasks.values() if t.enabled),
        "running_tasks": len(running_tasks),
        "total_executions": total_executions,
        "completed_executions": completed,
        "failed_executions": failed,
        "success_rate": completed / total_executions if total_executions > 0 else 0,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8098)
