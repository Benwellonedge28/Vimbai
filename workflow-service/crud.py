import json  # For serializing/deserializing complex Pydantic models to/from JSON strings for Neo4j properties
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from pydantic import BaseModel
from workflow_service.dependencies import book_id_var
from workflow_service.models import (
    WorkflowDefinitionCreate,
    WorkflowDefinitionInDB,
    WorkflowDefinitionUpdate,
    WorkflowInstanceCreate,
    WorkflowInstanceInDB,
    WorkflowInstanceUpdate,
    WorkflowStep,
    WorkflowTaskStatus,
)


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


# Helper function to convert Pydantic models to Neo4j-compatible dictionary (handles nested models)
def _to_neo4j_props(model_instance: BaseModel) -> Dict[str, Any]:
    data = model_instance.model_dump()
    # Convert nested Pydantic models to JSON strings for Neo4j storage
    for key, value in data.items():
        if isinstance(value, list) and value and all(isinstance(item, WorkflowStep) for item in value):
            data[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, list) and value and all(isinstance(item, WorkflowTaskStatus) for item in value):
            data[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, datetime):
            data[key] = value.isoformat()
    return data


# Helper function to reconstruct Pydantic models from Neo4j properties
def _from_neo4j_props(node_props: Dict[str, Any], model_class: BaseModel) -> BaseModel:
    props = node_props.copy()
    if "created_at" in props and isinstance(props["created_at"], str):
        props["created_at"] = datetime.fromisoformat(props["created_at"])
    if "updated_at" in props and isinstance(props["updated_at"], str):
        props["updated_at"] = datetime.fromisoformat(props["updated_at"])
    if "start_date" in props and isinstance(props["start_date"], str):
        props["start_date"] = datetime.fromisoformat(props["start_date"])
    if "end_date" in props and isinstance(props["end_date"], str):
        props["end_date"] = datetime.fromisoformat(props["end_date"])
    if "completed_at" in props and isinstance(props["completed_at"], str):
        props["completed_at"] = datetime.fromisoformat(props["completed_at"])

    # Reconstruct nested Pydantic models from JSON strings
    if "steps" in props and isinstance(props["steps"], str):
        props["steps"] = [WorkflowStep(**item) for item in json.loads(props["steps"])]
    if "tasks" in props and isinstance(props["tasks"], str):
        props["tasks"] = [WorkflowTaskStatus(**item) for item in json.loads(props["tasks"])]
    if "current_step_ids" in props and isinstance(props["current_step_ids"], str):
        props["current_step_ids"] = json.loads(props["current_step_ids"])

    return model_class(**props)


# --- WorkflowDefinition CRUD ---
async def create_workflow_definition(
    session: AsyncSession, definition_data: WorkflowDefinitionCreate
) -> WorkflowDefinitionInDB:
    definition_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(definition_data)
    props["id"] = definition_id
    props["book_id"] = book_id_var.get()
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    CREATE (wd:WorkflowDefinition $props)
    RETURN wd
    """
    result = await _run(session, query, props=props)
    record = await result.single()

    return _from_neo4j_props(record["wd"], WorkflowDefinitionInDB)


async def get_workflow_definition(session: AsyncSession, definition_id: str) -> Optional[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition {id: $definition_id})
    WHERE $book_id IS NULL OR wd.book_id = $book_id
    RETURN wd
    """
    result = await _run(session, query, definition_id=definition_id)
    record = await result.single()

    if record:
        return _from_neo4j_props(record["wd"], WorkflowDefinitionInDB)
    return None


async def get_workflow_definition_by_trigger(session: AsyncSession, trigger_event: str) -> List[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition {trigger_event: $trigger_event, is_active: true})
    WHERE $book_id IS NULL OR wd.book_id = $book_id
    RETURN wd
    """
    result = await _run(session, query, trigger_event=trigger_event)
    definitions = []
    async for record in result:
        definitions.append(_from_neo4j_props(record["wd"], WorkflowDefinitionInDB))
    return definitions


async def get_all_workflow_definitions(session: AsyncSession) -> List[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition)
    WHERE $book_id IS NULL OR wd.book_id = $book_id
    RETURN wd
    ORDER BY wd.name
    """
    result = await _run(session, query)
    definitions = []
    async for record in result:
        definitions.append(_from_neo4j_props(record["wd"], WorkflowDefinitionInDB))
    return definitions


async def update_workflow_definition(
    session: AsyncSession, definition_id: str, definition_data: WorkflowDefinitionUpdate
) -> Optional[WorkflowDefinitionInDB]:
    update_fields = definition_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_workflow_definition(session, definition_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    # Convert nested models to JSON string before update
    for key, value in update_fields.items():
        if isinstance(value, list) and all(isinstance(item, WorkflowStep) for item in value):
            update_fields[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, datetime):
            update_fields[key] = value.isoformat()

    set_clauses = [f"wd.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (wd:WorkflowDefinition {{id: $definition_id}})
    WHERE $book_id IS NULL OR wd.book_id = $book_id
    SET {set_query_part}
    RETURN wd
    """
    params = {"definition_id": definition_id, **update_fields}
    result = await _run(session, query, params)
    record = await result.single()

    if record:
        return await get_workflow_definition(session, definition_id)
    return None


async def delete_workflow_definition(session: AsyncSession, definition_id: str) -> bool:
    query = """
    MATCH (wd:WorkflowDefinition {id: $definition_id})
    WHERE $book_id IS NULL OR wd.book_id = $book_id
    DETACH DELETE wd
    """
    result = await _run(session, query, definition_id=definition_id)
    return result.consume().counters.nodes_deleted > 0


# --- WorkflowInstance CRUD ---
async def create_workflow_instance(
    session: AsyncSession, instance_data: WorkflowInstanceCreate
) -> WorkflowInstanceInDB:
    instance_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(instance_data)
    props["id"] = instance_id
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()
    # Initialize tasks list for the new instance
    props["tasks"] = json.dumps([])  # Start with no tasks
    props["book_id"] = book_id_var.get()

    query = """
    MATCH (wd:WorkflowDefinition {id: $workflow_definition_id})
    WHERE $book_id IS NULL OR wd.book_id = $book_id
    CREATE (wi:WorkflowInstance $props)
    CREATE (wi)-[:BASED_ON]->(wd)
    RETURN wi
    """
    result = await _run(session, query, workflow_definition_id=instance_data.workflow_definition_id, props=props)
    record = await result.single()

    if not record:
        return None  # definition not visible in this Book context
    return _from_neo4j_props(record["wi"], WorkflowInstanceInDB)


async def get_workflow_instance(session: AsyncSession, instance_id: str) -> Optional[WorkflowInstanceInDB]:
    query = """
    MATCH (wi:WorkflowInstance {id: $instance_id})
    WHERE $book_id IS NULL OR wi.book_id = $book_id
    RETURN wi
    """
    result = await _run(session, query, instance_id=instance_id)
    record = await result.single()

    if record:
        return _from_neo4j_props(record["wi"], WorkflowInstanceInDB)
    return None


async def get_workflow_instances_by_definition(session: AsyncSession, definition_id: str) -> List[WorkflowInstanceInDB]:
    query = """
    MATCH (wi:WorkflowInstance)-[:BASED_ON]->(wd:WorkflowDefinition {id: $definition_id})
    WHERE $book_id IS NULL OR wi.book_id = $book_id
    RETURN wi
    ORDER BY wi.start_date DESC
    """
    result = await _run(session, query, definition_id=definition_id)
    instances = []
    async for record in result:
        instances.append(_from_neo4j_props(record["wi"], WorkflowInstanceInDB))
    return instances


async def update_workflow_instance(
    session: AsyncSession, instance_id: str, instance_data: WorkflowInstanceUpdate
) -> Optional[WorkflowInstanceInDB]:
    update_fields = instance_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_workflow_instance(session, instance_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    # Convert nested models to JSON string before update
    for key, value in update_fields.items():
        if isinstance(value, list) and all(isinstance(item, WorkflowTaskStatus) for item in value):
            update_fields[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, datetime):
            update_fields[key] = value.isoformat()

    set_clauses = [f"wi.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (wi:WorkflowInstance {{id: $instance_id}})
    WHERE $book_id IS NULL OR wi.book_id = $book_id
    SET {set_query_part}
    RETURN wi
    """
    params = {"instance_id": instance_id, **update_fields}
    result = await _run(session, query, params)
    record = await result.single()

    if record:
        return await get_workflow_instance(session, instance_id)
    return None


async def delete_workflow_instance(session: AsyncSession, instance_id: str) -> bool:
    query = """
    MATCH (wi:WorkflowInstance {id: $instance_id})
    WHERE $book_id IS NULL OR wi.book_id = $book_id
    DETACH DELETE wi
    """
    result = await _run(session, query, instance_id=instance_id)
    return result.consume().counters.nodes_deleted > 0
