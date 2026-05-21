from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from workflow_service.models import (
    WorkflowDefinitionCreate, WorkflowDefinitionUpdate, WorkflowDefinitionInDB,
    WorkflowInstanceCreate, WorkflowInstanceUpdate, WorkflowInstanceInDB,
    WorkflowStep, WorkflowTaskStatus
)
from datetime import datetime
import uuid
import json # For serializing/deserializing complex Pydantic models to/from JSON strings for Neo4j properties

# Helper function to convert Pydantic models to Neo4j-compatible dictionary (handles nested models)
def _to_neo4j_props(model_instance: BaseModel) -> Dict[str, Any]:
    data = model_instance.model_dump()
    # Convert nested Pydantic models to JSON strings for Neo4j storage
    for key, value in data.items():
        if isinstance(value, list) and all(isinstance(item, WorkflowStep) for item in value):
            data[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, list) and all(isinstance(item, WorkflowTaskStatus) for item in value):
            data[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, datetime):
            data[key] = value.isoformat()
    return data

# Helper function to reconstruct Pydantic models from Neo4j properties
def _from_neo4j_props(node_props: Dict[str, Any], model_class: BaseModel) -> BaseModel:
    props = node_props.copy()
    if 'created_at' in props and isinstance(props['created_at'], str):
        props['created_at'] = datetime.fromisoformat(props['created_at'])
    if 'updated_at' in props and isinstance(props['updated_at'], str):
        props['updated_at'] = datetime.fromisoformat(props['updated_at'])
    if 'start_date' in props and isinstance(props['start_date'], str):
        props['start_date'] = datetime.fromisoformat(props['start_date'])
    if 'end_date' in props and isinstance(props['end_date'], str):
        props['end_date'] = datetime.fromisoformat(props['end_date'])
    if 'completed_at' in props and isinstance(props['completed_at'], str):
        props['completed_at'] = datetime.fromisoformat(props['completed_at'])

    # Reconstruct nested Pydantic models from JSON strings
    if 'steps' in props and isinstance(props['steps'], str):
        props['steps'] = [WorkflowStep(**item) for item in json.loads(props['steps'])]
    if 'tasks' in props and isinstance(props['tasks'], str):
        props['tasks'] = [WorkflowTaskStatus(**item) for item in json.loads(props['tasks'])]

    return model_class(**props)

# --- WorkflowDefinition CRUD ---
async def create_workflow_definition(session: AsyncSession, definition_data: WorkflowDefinitionCreate) -> WorkflowDefinitionInDB:
    definition_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    props = _to_neo4j_props(definition_data)
    props["id"] = definition_id
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    CREATE (wd:WorkflowDefinition $props)
    RETURN wd
    """
    result = await session.run(query, props=props)
    record = await result.single()
    
    return _from_neo4j_props(record["wd"], WorkflowDefinitionInDB)


async def get_workflow_definition(session: AsyncSession, definition_id: str) -> Optional[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition {id: $definition_id})
    RETURN wd
    """
    result = await session.run(query, definition_id=definition_id)
    record = await result.single()

    if record:
        return _from_neo4j_props(record["wd"], WorkflowDefinitionInDB)
    return None


async def get_workflow_definition_by_trigger(session: AsyncSession, trigger_event: str) -> List[WorkflowDefinitionInDB]:
    query = """
    MATCH (wd:WorkflowDefinition {trigger_event: $trigger_event, is_active: true})
    RETURN wd
    """
    result = await session.run(query, trigger_event=trigger_event)
    definitions = []
    async for record in result:
        definitions.append(_from_neo4j_props(record["wd"], WorkflowDefinitionInDB))
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
        definitions.append(_from_neo4j_props(record["wd"], WorkflowDefinitionInDB))
    return definitions


async def update_workflow_definition(session: AsyncSession, definition_id: str, definition_data: WorkflowDefinitionUpdate) -> Optional[WorkflowDefinitionInDB]:
    update_fields = definition_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_workflow_definition(session, definition_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
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


# --- WorkflowInstance CRUD ---
async def create_workflow_instance(session: AsyncSession, instance_data: WorkflowInstanceCreate) -> WorkflowInstanceInDB:
    instance_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    props = _to_neo4j_props(instance_data)
    props["id"] = instance_id
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()
    # Initialize tasks list for the new instance
    props["tasks"] = json.dumps([]) # Start with no tasks

    query = """
    MATCH (wd:WorkflowDefinition {id: $workflow_definition_id})
    CREATE (wi:WorkflowInstance $props)
    CREATE (wi)-[:BASED_ON]->(wd)
    RETURN wi
    """
    result = await session.run(query, workflow_definition_id=instance_data.workflow_definition_id, props=props)
    record = await result.single()

    return _from_neo4j_props(record["wi"], WorkflowInstanceInDB)


async def get_workflow_instance(session: AsyncSession, instance_id: str) -> Optional[WorkflowInstanceInDB]:
    query = """
    MATCH (wi:WorkflowInstance {id: $instance_id})
    RETURN wi
    """
    result = await session.run(query, instance_id=instance_id)
    record = await result.single()

    if record:
        return _from_neo4j_props(record["wi"], WorkflowInstanceInDB)
    return None


async def get_workflow_instances_by_definition(session: AsyncSession, definition_id: str) -> List[WorkflowInstanceInDB]:
    query = """
    MATCH (wi:WorkflowInstance)-[:BASED_ON]->(wd:WorkflowDefinition {id: $definition_id})
    RETURN wi
    ORDER BY wi.start_date DESC
    """
    result = await session.run(query, definition_id=definition_id)
    instances = []
    async for record in result:
        instances.append(_from_neo4j_props(record["wi"], WorkflowInstanceInDB))
    return instances


async def update_workflow_instance(session: AsyncSession, instance_id: str, instance_data: WorkflowInstanceUpdate) -> Optional[WorkflowInstanceInDB]:
    update_fields = instance_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_workflow_instance(session, instance_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
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
