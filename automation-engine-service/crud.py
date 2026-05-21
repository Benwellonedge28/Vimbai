from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from automation_engine_service.models import (
    AutomationTaskDefinitionCreate, AutomationTaskDefinitionUpdate, AutomationTaskDefinitionInDB,
    AutomationTaskInstanceCreate, AutomationTaskInstanceUpdate, AutomationTaskInstanceInDB,
    AutomationLogCreate, AutomationLogInDB
)
from datetime import datetime
import uuid

# --- AutomationTaskDefinition CRUD ---
async def create_automation_task_definition(session: AsyncSession, definition_data: AutomationTaskDefinitionCreate) -> AutomationTaskDefinitionInDB:
    definition_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $owner_user_id})
    CREATE (atd:AutomationTaskDefinition {
        id: $id,
        name: $name,
        description: $description,
        service_target: $service_target,
        endpoint_path: $endpoint_path,
        http_method: $http_method,
        payload_template: $payload_template,
        schedule_type: $schedule_type,
        cron_schedule: $cron_schedule,
        interval_seconds: $interval_seconds,
        is_active: $is_active,
        last_executed_at: datetime($last_executed_at) ON CREATE NULL,
        next_execution_at: datetime($next_execution_at) ON CREATE NULL,
        owner_user_id: $owner_user_id,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_AUTOMATION_DEFINITION]->(atd)
    RETURN atd
    """
    params = definition_data.model_dump()
    params["id"] = definition_id
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()
    if params["last_executed_at"]:
        params["last_executed_at"] = params["last_executed_at"].isoformat()
    if params["next_execution_at"]:
        params["next_execution_at"] = params["next_execution_at"].isoformat()

    result = await session.run(query, params)
    record = await result.single()
    atd_node = record["atd"]

    return AutomationTaskDefinitionInDB(
        id=atd_node["id"],
        name=atd_node["name"],
        description=atd_node["description"],
        service_target=atd_node["service_target"],
        endpoint_path=atd_node["endpoint_path"],
        http_method=atd_node["http_method"],
        payload_template=atd_node["payload_template"],
        schedule_type=atd_node["schedule_type"],
        cron_schedule=atd_node["cron_schedule"],
        interval_seconds=atd_node["interval_seconds"],
        is_active=atd_node["is_active"],
        last_executed_at=datetime.fromisoformat(atd_node["last_executed_at"].iso_format()) if atd_node.get("last_executed_at") else None,
        next_execution_at=datetime.fromisoformat(atd_node["next_execution_at"].iso_format()) if atd_node.get("next_execution_at") else None,
        owner_user_id=atd_node["owner_user_id"],
        created_at=datetime.fromisoformat(atd_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(atd_node["updated_at"].iso_format()),
    )

async def get_automation_task_definition(session: AsyncSession, definition_id: str) -> Optional[AutomationTaskDefinitionInDB]:
    query = """
    MATCH (atd:AutomationTaskDefinition {id: $definition_id})
    RETURN atd
    """
    result = await session.run(query, definition_id=definition_id)
    record = await result.single()

    if record:
        atd_node = record["atd"]
        return AutomationTaskDefinitionInDB(
            id=atd_node["id"],
            name=atd_node["name"],
            description=atd_node["description"],
            service_target=atd_node["service_target"],
            endpoint_path=atd_node["endpoint_path"],
            http_method=atd_node["http_method"],
            payload_template=atd_node["payload_template"],
            schedule_type=atd_node["schedule_type"],
            cron_schedule=atd_node["cron_schedule"],
            interval_seconds=atd_node["interval_seconds"],
            is_active=atd_node["is_active"],
            last_executed_at=datetime.fromisoformat(atd_node["last_executed_at"].iso_format()) if atd_node.get("last_executed_at") else None,
            next_execution_at=datetime.fromisoformat(atd_node["next_execution_at"].iso_format()) if atd_node.get("next_execution_at") else None,
            owner_user_id=atd_node["owner_user_id"],
            created_at=datetime.fromisoformat(atd_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(atd_node["updated_at"].iso_format()),
        )
    return None

async def get_all_automation_task_definitions(session: AsyncSession, owner_user_id: Optional[str] = None) -> List[AutomationTaskDefinitionInDB]:
    query = """
    MATCH (atd:AutomationTaskDefinition)
    WHERE ($owner_user_id IS NULL OR atd.owner_user_id = $owner_user_id)
    RETURN atd
    ORDER BY atd.name
    """
    result = await session.run(query, owner_user_id=owner_user_id)
    definitions = []
    async for record in result:
        atd_node = record["atd"]
        definitions.append(AutomationTaskDefinitionInDB(
            id=atd_node["id"],
            name=atd_node["name"],
            description=atd_node["description"],
            service_target=atd_node["service_target"],
            endpoint_path=atd_node["endpoint_path"],
            http_method=atd_node["http_method"],
            payload_template=atd_node["payload_template"],
            schedule_type=atd_node["schedule_type"],
            cron_schedule=atd_node["cron_schedule"],
            interval_seconds=atd_node["interval_seconds"],
            is_active=atd_node["is_active"],
            last_executed_at=datetime.fromisoformat(atd_node["last_executed_at"].iso_format()) if atd_node.get("last_executed_at") else None,
            next_execution_at=datetime.fromisoformat(atd_node["next_execution_at"].iso_format()) if atd_node.get("next_execution_at") else None,
            owner_user_id=atd_node["owner_user_id"],
            created_at=datetime.fromisoformat(atd_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(atd_node["updated_at"].iso_format()),
        ))
    return definitions

async def update_automation_task_definition(session: AsyncSession, definition_id: str, definition_data: AutomationTaskDefinitionUpdate) -> Optional[AutomationTaskDefinitionInDB]:
    update_fields = definition_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_automation_task_definition(session, definition_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "last_executed_at" in update_fields and update_fields["last_executed_at"]:
        update_fields["last_executed_at"] = update_fields["last_executed_at"].isoformat()
    if "next_execution_at" in update_fields and update_fields["next_execution_at"]:
        update_fields["next_execution_at"] = update_fields["next_execution_at"].isoformat()

    set_clauses = [f"atd.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (atd:AutomationTaskDefinition {{id: $definition_id}})
    SET {set_query_part}
    RETURN atd
    """
    params = {"definition_id": definition_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_automation_task_definition(session, definition_id)
    return None

async def delete_automation_task_definition(session: AsyncSession, definition_id: str) -> bool:
    query = """
    MATCH (atd:AutomationTaskDefinition {id: $definition_id})
    DETACH DELETE atd
    """
    result = await session.run(query, definition_id=definition_id)
    return result.consume().counters.nodes_deleted > 0

# --- AutomationTaskInstance CRUD ---
async def create_automation_task_instance(session: AsyncSession, instance_data: AutomationTaskInstanceCreate) -> AutomationTaskInstanceInDB:
    instance_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (atd:AutomationTaskDefinition {id: $task_definition_id})
    CREATE (ati:AutomationTaskInstance {
        id: $id,
        task_definition_id: $task_definition_id,
        status: $status,
        start_time: datetime($start_time),
        end_time: datetime($end_time) ON CREATE NULL,
        output: $output,
        error_message: $error_message,
        triggered_by: $triggered_by,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (ati)-[:INSTANTIATES]->(atd)
    RETURN ati
    """
    params = instance_data.model_dump()
    params["id"] = instance_id
    params["start_time"] = params["start_time"].isoformat()
    if params["end_time"]:
        params["end_time"] = params["end_time"].isoformat()
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    ati_node = record["ati"]

    return AutomationTaskInstanceInDB(
        id=ati_node["id"],
        task_definition_id=ati_node["task_definition_id"],
        status=ati_node["status"],
        start_time=datetime.fromisoformat(ati_node["start_time"].iso_format()),
        end_time=datetime.fromisoformat(ati_node["end_time"].iso_format()) if ati_node.get("end_time") else None,
        output=ati_node["output"],
        error_message=ati_node["error_message"],
        triggered_by=ati_node["triggered_by"],
        created_at=datetime.fromisoformat(ati_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(ati_node["updated_at"].iso_format()),
    )

async def get_automation_task_instance(session: AsyncSession, instance_id: str) -> Optional[AutomationTaskInstanceInDB]:
    query = """
    MATCH (ati:AutomationTaskInstance {id: $instance_id})
    RETURN ati
    """
    result = await session.run(query, instance_id=instance_id)
    record = await result.single()

    if record:
        ati_node = record["ati"]
        return AutomationTaskInstanceInDB(
            id=ati_node["id"],
            task_definition_id=ati_node["task_definition_id"],
            status=ati_node["status"],
            start_time=datetime.fromisoformat(ati_node["start_time"].iso_format()),
            end_time=datetime.fromisoformat(ati_node["end_time"].iso_format()) if ati_node.get("end_time") else None,
            output=ati_node["output"],
            error_message=ati_node["error_message"],
            triggered_by=ati_node["triggered_by"],
            created_at=datetime.fromisoformat(ati_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(ati_node["updated_at"].iso_format()),
        )
    return None

async def get_all_automation_task_instances_for_definition(session: AsyncSession, definition_id: str) -> List[AutomationTaskInstanceInDB]:
    query = """
    MATCH (atd:AutomationTaskDefinition {id: $definition_id})<-[:INSTANTIATES]-(ati:AutomationTaskInstance)
    RETURN ati
    ORDER BY ati.start_time DESC
    """
    result = await session.run(query, definition_id=definition_id)
    instances = []
    async for record in result:
        ati_node = record["ati"]
        instances.append(AutomationTaskInstanceInDB(
            id=ati_node["id"],
            task_definition_id=ati_node["task_definition_id"],
            status=ati_node["status"],
            start_time=datetime.fromisoformat(ati_node["start_time"].iso_format()),
            end_time=datetime.fromisoformat(ati_node["end_time"].iso_format()) if ati_node.get("end_time") else None,
            output=ati_node["output"],
            error_message=ati_node["error_message"],
            triggered_by=ati_node["triggered_by"],
            created_at=datetime.fromisoformat(ati_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(ati_node["updated_at"].iso_format()),
        ))
    return instances


async def update_automation_task_instance(session: AsyncSession, instance_id: str, instance_data: AutomationTaskInstanceUpdate) -> Optional[AutomationTaskInstanceInDB]:
    update_fields = instance_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_automation_task_instance(session, instance_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "end_time" in update_fields and update_fields["end_time"]:
        update_fields["end_time"] = update_fields["end_time"].isoformat()

    set_clauses = [f"ati.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (ati:AutomationTaskInstance {{id: $instance_id}})
    SET {set_query_part}
    RETURN ati
    """
    params = {"instance_id": instance_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_automation_task_instance(session, instance_id)
    return None

# --- AutomationLog CRUD ---
async def create_automation_log(session: AsyncSession, log_data: AutomationLogCreate) -> AutomationLogInDB:
    log_id = str(uuid.uuid4())
    timestamp = datetime.utcnow()

    query = """
    MATCH (ati:AutomationTaskInstance {id: $instance_id})
    CREATE (al:AutomationLog {n        id: $id,
        instance_id: $instance_id,
        log_level: $log_level,
        message: $message,
        details: $details,
        timestamp: datetime($timestamp)
    })
    CREATE (ati)-[:HAS_LOG]->(al)
    RETURN al
    """
    params = log_data.model_dump()
    params["id"] = log_id
    params["timestamp"] = timestamp.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    al_node = record["al"]

    return AutomationLogInDB(
        id=al_node["id"],
        instance_id=al_node["instance_id"],
        log_level=al_node["log_level"],
        message=al_node["message"],
        details=al_node["details"],
        timestamp=datetime.fromisoformat(al_node["timestamp"].iso_format()),
    )

async def get_automation_logs_for_instance(session: AsyncSession, instance_id: str) -> List[AutomationLogInDB]:
    query = """
    MATCH (ati:AutomationTaskInstance {id: $instance_id})-[:HAS_LOG]->(al:AutomationLog)
    RETURN al
    ORDER BY al.timestamp ASC
    """
    result = await session.run(query, instance_id=instance_id)
    logs = []
    async for record in result:
        al_node = record["al"]
        logs.append(AutomationLogInDB(
            id=al_node["id"],
            instance_id=al_node["instance_id"],
            log_level=al_node["log_level"],
            message=al_node["message"],
            details=al_node["details"],
            timestamp=datetime.fromisoformat(al_node["timestamp"].iso_format()),
        ))
    return logs

