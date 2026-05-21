from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from workflow_service.models import (
    WorkflowDefinitionCreate, WorkflowDefinitionUpdate, WorkflowDefinitionInDB,
    WorkflowInstanceCreate, WorkflowInstanceUpdate, WorkflowInstanceInDB,
    WorkflowStep, WorkflowTaskStatus
)
from datetime import datetime
import uuid

# --- Workflow Definition CRUD ---
async def create_workflow_definition(session: AsyncSession, definition_data: WorkflowDefinitionCreate) -> WorkflowDefinitionInDB:
    definition_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Convert WorkflowStep objects to dictionaries for storage
    steps_data = [step.model_dump() for step in definition_data.steps]

    query = """
    CREATE (wd:WorkflowDefinition {
        id: $id,
        name: $name,
        description: $description,
        trigger_event: $trigger_event,
        steps: $steps,
        is_active: $is_active,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    RETURN wd
    """
    params = definition_data.model_dump()
    params["id"] = definition_id
    params["steps"] = steps_data  # Store steps as a list of dictionaries
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    wd_node = record["wd"]

    # Reconstruct steps from stored dictionaries
    reconstructed_steps = [WorkflowStep(**s) for s in wd_node["steps"]]

    return WorkflowDefinitionInDB(
        id=wd_node["id"],
        name=wd_node["name"],
        description=wd_node["description"],
        trigger_event=wd_node["trigger_event"],
        steps=reconstructed_steps,
        is_active=wd_node["is_active"],
        created_at=datetime.fromisoformat(wd_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(wd_node["updated_at"].iso_format()),
    )

async def get_workflow_definition(session: AsyncSession, definition_id: str) -> Optional[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition {id: $definition_id})
    RETURN wd
    """
    result = await session.run(query, definition_id=definition_id)
    record = await result.single()

    if record:
        wd_node = record["wd"]
        reconstructed_steps = [WorkflowStep(**s) for s in wd_node["steps"]]
        return WorkflowDefinitionInDB(
            id=wd_node["id"],
            name=wd_node["name"],
            description=wd_node["description"],
            trigger_event=wd_node["trigger_event"],
            steps=reconstructed_steps,
            is_active=wd_node["is_active"],
            created_at=datetime.fromisoformat(wd_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(wd_node["updated_at"].iso_format()),
        )
    return None

async def get_workflow_definitions_by_trigger_event(session: AsyncSession, trigger_event: str) -> List[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition {trigger_event: $trigger_event, is_active: true})
    RETURN wd
    """
    result = await session.run(query, trigger_event=trigger_event)
    definitions = []
    async for record in result:
        wd_node = record["wd"]
        reconstructed_steps = [WorkflowStep(**s) for s in wd_node["steps"]]
        definitions.append(WorkflowDefinitionInDB(
            id=wd_node["id"],
            name=wd_node["name"],
            description=wd_node["description"],
            trigger_event=wd_node["trigger_event"],
            steps=reconstructed_steps,
            is_active=wd_node["is_active"],
            created_at=datetime.fromisoformat(wd_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(wd_node["updated_at"].iso_format()),
        ))
    return definitions

async def get_all_workflow_definitions(session: AsyncSession) -> List[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition)
    RETURN wd
    ORDER BY wd.name
    """
    result = await session.run(query)
    definitions = []
    async for record in result:
        wd_node = record["wd"]
        reconstructed_steps = [WorkflowStep(**s) for s in wd_node["steps"]]
        definitions.append(WorkflowDefinitionInDB(
            id=wd_node["id"],
            name=wd_node["name"],
            description=wd_node["description"],
            trigger_event=wd_node["trigger_event"],
            steps=reconstructed_steps,
            is_active=wd_node["is_active"],
            created_at=datetime.fromisoformat(wd_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(wd_node["updated_at"].iso_format()),
        ))
    return definitions


async def update_workflow_definition(session: AsyncSession, definition_id: str, definition_data: WorkflowDefinitionUpdate) -> Optional[WorkflowDefinitionInDB]:
    update_fields = definition_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_workflow_definition(session, definition_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "steps" in update_fields:
        update_fields["steps"] = [step.model_dump() for step in definition_data.steps] # Convert steps to dicts

    set_clauses = [f"wd.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (wd:WorkflowDefinition {{id: $definition_id}})
    SET {set_query_part}
    RETURN wd
    """
    params = {"definition_id": definition_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_workflow_definition(session, definition_id)
    return None

async def delete_workflow_definition(session: AsyncSession, definition_id: str) -> bool:
    query = """
    MATCH (wd:WorkflowDefinition {id: $definition_id})
    DETACH DELETE wd
    """
    result = await session.run(query, definition_id=definition_id)
    return result.consume().counters.nodes_deleted > 0

# --- Workflow Instance CRUD ---
async def create_workflow_instance(session: AsyncSession, instance_data: WorkflowInstanceCreate) -> WorkflowInstanceInDB:
    instance_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (wd:WorkflowDefinition {id: $workflow_definition_id})
    CREATE (wi:WorkflowInstance {
        id: $id,
        workflow_definition_id: $workflow_definition_id,
        triggered_by_event: $triggered_by_event,
        status: $status,
        current_step_ids: $current_step_ids,
        context: $context,
        start_date: datetime($start_date),
        end_date: $end_date,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (wi)-[:BASED_ON]->(wd)
    RETURN wi
    """
    params = instance_data.model_dump()
    params["id"] = instance_id
    params["triggered_by_event"] = str(params["triggered_by_event"]) # Ensure string for storage
    params["current_step_ids"] = [] # Start with no active steps
    params["start_date"] = params["start_date"].isoformat()
    if params["end_date"]:
        params["end_date"] = params["end_date"].isoformat()
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    wi_node = record["wi"]

    return WorkflowInstanceInDB(
        id=wi_node["id"],
        workflow_definition_id=wi_node["workflow_definition_id"],
        triggered_by_event=wi_node["triggered_by_event"],
        status=wi_node["status"],
        current_step_ids=wi_node["current_step_ids"],
        context=wi_node["context"],
        start_date=datetime.fromisoformat(wi_node["start_date"].iso_format()),
        end_date=datetime.fromisoformat(wi_node["end_date"].iso_format()) if wi_node.get("end_date") else None,
        tasks=[], # Tasks will be added separately
        created_at=datetime.fromisoformat(wi_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(wi_node["updated_at"].iso_format()),
    )

async def get_workflow_instance(session: AsyncSession, instance_id: str) -> Optional[WorkflowInstanceInDB]:
    query = """
    MATCH (wi:WorkflowInstance {id: $instance_id})
    OPTIONAL MATCH (wi)-[:HAS_TASK_STATUS]->(wts:WorkflowTaskStatus)
    RETURN wi, COLLECT(wts) AS tasks_data
    """
    params = {"instance_id": instance_id}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        wi_node = record["wi"]
        tasks_data = record["tasks_data"]
        
        reconstructed_tasks = []
        for task_node in tasks_data:
            if task_node:
                reconstructed_tasks.append(WorkflowTaskStatus(
                    task_id=task_node["task_id"],
                    step_id=task_node["step_id"],
                    status=task_node["status"],
                    assigned_to_user_id=task_node.get("assigned_to_user_id"),
                    assigned_to_role=task_node.get("assigned_to_role"),
                    completed_by_user_id=task_node.get("completed_by_user_id"),
                    completed_at=datetime.fromisoformat(task_node["completed_at"].iso_format()) if task_node.get("completed_at") else None,
                    comments=task_node.get("comments"),
                    payload=task_node["payload"],
                ))

        return WorkflowInstanceInDB(
            id=wi_node["id"],
            workflow_definition_id=wi_node["workflow_definition_id"],
            triggered_by_event=wi_node["triggered_by_event"],
            status=wi_node["status"],
            current_step_ids=wi_node["current_step_ids"],
            context=wi_node["context"],
            start_date=datetime.fromisoformat(wi_node["start_date"].iso_format()),
            end_date=datetime.fromisoformat(wi_node["end_date"].iso_format()) if wi_node.get("end_date") else None,
            tasks=reconstructed_tasks,
            created_at=datetime.fromisoformat(wi_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(wi_node["updated_at"].iso_format()),
        )
    return None


async def update_workflow_instance(session: AsyncSession, instance_id: str, instance_data: WorkflowInstanceUpdate) -> Optional[WorkflowInstanceInDB]:
    update_fields = instance_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_workflow_instance(session, instance_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "start_date" in update_fields:
        update_fields["start_date"] = update_fields["start_date"].isoformat()
    if "end_date" in update_fields and update_fields["end_date"]:
        update_fields["end_date"] = update_fields["end_date"].isoformat()

    set_clauses = [f"wi.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (wi:WorkflowInstance {{id: $instance_id}})
    SET {set_query_part}
    RETURN wi
    """
    params = {"instance_id": instance_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_workflow_instance(session, instance_id)
    return None

async def delete_workflow_instance(session: AsyncSession, instance_id: str) -> bool:
    query = """
    MATCH (wi:WorkflowInstance {id: $instance_id})
    DETACH DELETE wi
    """
    result = await session.run(query, instance_id=instance_id)
    return result.consume().counters.nodes_deleted > 0


async def add_task_status_to_instance(session: AsyncSession, instance_id: str, task_status: WorkflowTaskStatus) -> Optional[WorkflowInstanceInDB]:
    task_status_data = task_status.model_dump()
    task_status_data["completed_at"] = task_status_data["completed_at"].isoformat() if task_status_data["completed_at"] else None

    query = """
    MATCH (wi:WorkflowInstance {id: $instance_id})
    CREATE (wts:WorkflowTaskStatus {
        task_id: $task_id,
        step_id: $step_id,
        status: $status,
        assigned_to_user_id: $assigned_to_user_id,
        assigned_to_role: $assigned_to_role,
        completed_by_user_id: $completed_by_user_id,
        completed_at: datetime($completed_at),
        comments: $comments,
        payload: $payload
    })
    CREATE (wi)-[:HAS_TASK_STATUS]->(wts)
    RETURN wi
    """
    params = {"instance_id": instance_id, **task_status_data}
    result = await session.run(query, params)
    record = await result.single()
    if record:
        return await get_workflow_instance(session, instance_id)
    return None
