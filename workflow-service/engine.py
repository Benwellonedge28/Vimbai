from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from workflow_service import crud, models
from datetime import datetime
import uuid
import json

class WorkflowEngineException(Exception):
    """Custom exception for workflow engine errors."""
    pass

class WorkflowEngine:
    def __init__(self, db_session: AsyncSession):
        self.db_session = db_session

    async def _evaluate_next_steps(self, instance: models.WorkflowInstanceInDB, definition: models.WorkflowDefinitionInDB,
                                   completed_step_id: str, action: Optional[str] = None) -> List[models.WorkflowStep]:
        """Determines the next steps based on the completed step and action."""
        for step_def in definition.steps:
            if step_def.step_id == completed_step_id:
                if action == "reject" and step_def.on_rejection_steps:
                    next_step_ids = step_def.on_rejection_steps
                else:
                    next_step_ids = step_def.next_steps

                next_steps = [s for s in definition.steps if s.step_id in next_step_ids]
                return next_steps
        return []

    async def start_workflow_instance(self, definition_id: str, triggered_by_event: Dict[str, Any], context: Dict[str, Any]) -> models.WorkflowInstanceInDB:
        """
        Starts a new workflow instance based on a definition.
        Initializes the first tasks based on the workflow definition's start.
        """
        definition = await crud.get_workflow_definition(self.db_session, definition_id)
        if not definition:
            raise NotFoundError(f"Workflow Definition {definition_id} not found.")
        if not definition.is_active:
            raise WorkflowEngineException(f"Workflow Definition {definition_id} is not active.")

        # Create the workflow instance
        instance_create_data = models.WorkflowInstanceCreate(
            workflow_definition_id=definition_id,
            triggered_by_event=json.dumps(triggered_by_event), # Store event payload as JSON string
            context=context,
            status="running"
        )
        instance = await crud.create_workflow_instance(self.db_session, instance_create_data)

        # Find the first step(s) of the workflow (assuming the first step in the list is the entry point, or a specific "start" step)
        if not definition.steps:
            raise WorkflowEngineException(f"Workflow Definition {definition_id} has no steps defined.")
        
        # For simplicity, start with the first step in the definition
        first_step = definition.steps[0] 
        
        # Create the initial task(s)
        new_task = models.WorkflowTaskStatus(
            task_id=str(uuid.uuid4()),
            step_id=first_step.step_id,
            status="pending",
            assigned_to_role=first_step.assignee_role,
            assigned_to_user_id=first_step.assignee_user_id,
            payload=first_step.config # Pass step config as task payload
        )
        instance.tasks.append(new_task)
        instance.current_step_ids = [new_task.task_id]

        updated_instance = await crud.update_workflow_instance(self.db_session, instance.id!, models.WorkflowInstanceUpdate(
            current_step_ids=instance.current_step_ids,
            tasks=instance.tasks
        ))
        if not updated_instance:
            raise WorkflowEngineException(f"Failed to update workflow instance {instance.id} with initial tasks.")
        
        return updated_instance

    async def complete_workflow_task(self, instance_id: str, task_completion_data: models.WorkflowTaskCompletion) -> models.WorkflowInstanceInDB:
        """
        Completes a specific task within a workflow instance.
        Transitions the workflow to the next appropriate step(s).
        """
        instance = await crud.get_workflow_instance(self.db_session, instance_id)
        if not instance:
            raise NotFoundError(f"Workflow Instance {instance_id} not found.")

        definition = await crud.get_workflow_definition(self.db_session, instance.workflow_definition_id)
        if not definition:
            raise NotFoundError(f"Workflow Definition {instance.workflow_definition_id} not found for instance {instance_id}.")

        task_status_to_complete: Optional[models.WorkflowTaskStatus] = None
        for task in instance.tasks:
            if task.task_id == task_completion_data.task_id:
                task_status_to_complete = task
                break
        
        if not task_status_to_complete:
            raise NotFoundError(f"Workflow Task {task_completion_data.task_id} not found in instance {instance_id}.")
        if task_status_to_complete.status != "pending" and task_status_to_complete.status != "in_progress":
            raise WorkflowEngineException(f"Workflow Task {task_completion_data.task_id} is already {task_status_to_complete.status}.")

        # Update the task status
        task_status_to_complete.status = task_completion_data.action # 'approve', 'reject', 'complete'
        task_status_to_complete.completed_by_user_id = task_completion_data.completed_by_user_id
        task_status_to_complete.completed_at = datetime.utcnow()
        task_status_to_complete.comments = task_completion_data.comments
        
        # Remove from current_step_ids
        instance.current_step_ids = [tid for tid in instance.current_step_ids if tid != task_completion_data.task_id]

        # Evaluate next steps
        next_steps_defs = await self._evaluate_next_steps(instance, definition, task_status_to_complete.step_id, task_completion_data.action)
        
        new_tasks_created = []
        for next_step_def in next_steps_defs:
            new_task = models.WorkflowTaskStatus(
                task_id=str(uuid.uuid4()),
                step_id=next_step_def.step_id,
                status="pending",
                assigned_to_role=next_step_def.assignee_role,
                assigned_to_user_id=next_step_def.assignee_user_id,
                payload=next_step_def.config
            )
            instance.tasks.append(new_task)
            instance.current_step_ids.append(new_task.task_id)
            new_tasks_created.append(new_task.task_id)

        # Check if workflow is completed (no more current_step_ids)
        if not instance.current_step_ids:
            instance.status = "completed"
            instance.end_date = datetime.utcnow()
        elif instance.status != "failed" and instance.status != "cancelled": # Ensure status isn't overwritten if already failed/cancelled
            instance.status = "running" # Still running if there are new steps

        updated_instance = await crud.update_workflow_instance(self.db_session, instance.id!, models.WorkflowInstanceUpdate(
            status=instance.status,
            current_step_ids=instance.current_step_ids,
            tasks=instance.tasks,
            end_date=instance.end_date
        ))
        if not updated_instance:
            raise WorkflowEngineException(f"Failed to update workflow instance {instance.id} after task completion.")
        
        return updated_instance
