from pydantic import BaseModel, Field
from typing import Optional, List, Dict, Any, Literal
from datetime import datetime, timedelta

# --- Automation Task Definition Models ---
class AutomationTaskDefinitionBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Name of the automated task.")
    description: Optional[str] = Field(None, max_length=500, description="Description of what the task does.")
    service_target: str = Field(..., description="The target service for the automation (e.g., 'accounting', 'finance', 'banking').")
    endpoint_path: str = Field(..., description="The API endpoint path to call on the target service (e.g., '/journal-entries/reconcile-all').")
    http_method: Literal["GET", "POST", "PUT", "DELETE"] = Field("POST")
    payload_template: Dict[str, Any] = Field({}, description="JSON template for the request body.")
    schedule_type: Literal["manual", "cron", "interval"] = Field("manual")
    cron_schedule: Optional[str] = Field(None, description="Cron string for scheduling (e.g., '0 0 * * *').")
    interval_seconds: Optional[int] = Field(None, description="Interval in seconds for repeated execution.")
    is_active: bool = True
    last_executed_at: Optional[datetime] = None
    next_execution_at: Optional[datetime] = None
    owner_user_id: str = Field(..., description="User ID who owns/created this automation.")

class AutomationTaskDefinitionCreate(AutomationTaskDefinitionBase):
    pass

class AutomationTaskDefinitionUpdate(BaseModel):
    name: Optional[str] = None
    description: Optional[str] = None
    service_target: Optional[str] = None
    endpoint_path: Optional[str] = None
    http_method: Optional[Literal["GET", "POST", "PUT", "DELETE"]] = None
    payload_template: Optional[Dict[str, Any]] = None
    schedule_type: Optional[Literal["manual", "cron", "interval"]] = None
    cron_schedule: Optional[str] = None
    interval_seconds: Optional[int] = None
    is_active: Optional[bool] = None

class AutomationTaskDefinitionInDB(AutomationTaskDefinitionBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Automation Task Instance Models (running tasks) ---
class AutomationTaskInstanceBase(BaseModel):
    task_definition_id: str = Field(..., description="ID of the AutomationTaskDefinition this instance is based on.")
    status: Literal["pending", "running", "completed", "failed", "cancelled"] = Field("pending")
    start_time: datetime = Field(default_factory=datetime.utcnow)
    end_time: Optional[datetime] = None
    output: Optional[Dict[str, Any]] = Field(None, description="Output of the task execution.")
    error_message: Optional[str] = Field(None, max_length=1000, description="Error message if task failed.")
    triggered_by: Literal["schedule", "manual", "event"] = Field("manual")

class AutomationTaskInstanceCreate(AutomationTaskInstanceBase):
    pass

class AutomationTaskInstanceUpdate(BaseModel):
    status: Optional[Literal["pending", "running", "completed", "failed", "cancelled"]] = None
    end_time: Optional[datetime] = None
    output: Optional[Dict[str, Any]] = None
    error_message: Optional[str] = None

class AutomationTaskInstanceInDB(AutomationTaskInstanceBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Automation Log Models ---
class AutomationLogBase(BaseModel):
    instance_id: str = Field(..., description="ID of the AutomationTaskInstance this log belongs to.")
    log_level: Literal["INFO", "WARN", "ERROR", "DEBUG"] = Field("INFO")
    message: str = Field(..., description="Log message.")
    details: Optional[Dict[str, Any]] = Field(None, description="Additional log details.")

class AutomationLogCreate(AutomationLogBase):
    pass

class AutomationLogInDB(AutomationLogBase):
    id: str = Field(..., example="uuid-string-for-node")
    timestamp: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True
