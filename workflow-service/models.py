from pydantic import BaseModel, Field
from typing import Optional, List, Dict, Any, Literal
from datetime import datetime

# --- Workflow Definition Models ---
class WorkflowStep(BaseModel):
    step_id: str = Field(..., description="Unique ID for this step within the workflow definition.")
    name: str = Field(..., min_length=3, max_length=100, description="Name of the workflow step.")
    step_type: Literal["approval", "notification", "action", "conditional", "parallel"] = Field(..., description="Type of workflow step.")
    assignee_role: Optional[str] = Field(None, description="Role required to complete this step (e.g., 'Accountant', 'FinanceManager').")
    assignee_user_id: Optional[str] = Field(None, description="Specific user ID assigned to this step.")
    description: Optional[str] = Field(None, max_length=500, description="Description of the step.")
    config: Dict[str, Any] = Field({}, description="JSON configuration for the step (e.g., notification message, action details).")
    next_steps: List[str] = Field([], description="List of step_ids for next steps to execute on completion.")
    on_rejection_steps: List[str] = Field([], description="List of step_ids for steps to execute on rejection (for approval steps).")

class WorkflowDefinitionBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Name of the workflow definition (e.g., 'Invoice Approval').")
    description: Optional[str] = Field(None, max_length=500, description="Description of the workflow.")
    trigger_event: str = Field(..., description="Event that triggers this workflow (e.g., 'JournalEntryCreated', 'VendorBillReceived').")
    steps: List[WorkflowStep] = Field([], description="Ordered list of steps in the workflow.")
    is_active: bool = True

class WorkflowDefinitionCreate(WorkflowDefinitionBase):
    pass

class WorkflowDefinitionUpdate(WorkflowDefinitionBase):
    name: Optional[str] = None
    description: Optional[str] = None
    trigger_event: Optional[str] = None
    steps: Optional[List[WorkflowStep]] = None
    is_active: Optional[bool] = None

class WorkflowDefinitionInDB(WorkflowDefinitionBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Workflow Instance Models (running workflows) ---
class WorkflowTaskStatus(BaseModel):
    task_id: str = Field(..., description="Unique ID of the task within this instance.")
    step_id: str = Field(..., description="Reference to the step_id in the WorkflowDefinition.")
    status: Literal["pending", "in_progress", "completed", "rejected", "cancelled"] = "pending"
    assigned_to_user_id: Optional[str] = None
    assigned_to_role: Optional[str] = None
    completed_by_user_id: Optional[str] = None
    completed_at: Optional[datetime] = None
    comments: Optional[str] = None
    payload: Dict[str, Any] = Field({}, description="Dynamic data associated with this task.")

class WorkflowInstanceBase(BaseModel):
    workflow_definition_id: str = Field(..., description="ID of the WorkflowDefinition this instance is based on.")
    triggered_by_event: str = Field(..., description="The actual event payload that triggered this instance.")
    status: Literal["running", "paused", "completed", "failed", "cancelled"] = "running"
    current_step_ids: List[str] = Field([], description="IDs of currently active workflow tasks.")
    context: Dict[str, Any] = Field({}, description="Dynamic context/data for this workflow instance.")
    start_date: datetime = Field(default_factory=datetime.utcnow)
    end_date: Optional[datetime] = None

class WorkflowInstanceCreate(WorkflowInstanceBase):
    pass

class WorkflowInstanceUpdate(WorkflowInstanceBase):
    workflow_definition_id: Optional[str] = None
    triggered_by_event: Optional[str] = None
    status: Optional[Literal["running", "paused", "completed", "failed", "cancelled"]] = None
    current_step_ids: Optional[List[str]] = None
    context: Optional[Dict[str, Any]] = None
    start_date: Optional[datetime] = None
    end_date: Optional[datetime] = None

class WorkflowInstanceInDB(WorkflowInstanceBase):
    id: str = Field(..., example="uuid-string-for-node")
    tasks: List[WorkflowTaskStatus] = Field([], description="List of all tasks created for this instance.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Response Models ---
class WorkflowTriggerResponse(BaseModel):
    instance_id: str
    message: str

class WorkflowTaskAssignment(BaseModel):
    task_id: str
    assigned_to_user_id: str
    assigned_to_role: Optional[str] = None

class WorkflowTaskCompletion(BaseModel):
    task_id: str
    completed_by_user_id: str
    action: Literal["approve", "reject", "complete"]
    comments: Optional[str] = None

class WorkflowSummary(BaseModel):
    id: str
    name: str
    status: str
    current_step: Optional[str] = None
    last_updated: datetime
