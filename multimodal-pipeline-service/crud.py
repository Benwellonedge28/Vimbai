import json  # For serializing/deserializing complex Pydantic models to/from JSON strings for Neo4j properties
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from multimodal_pipeline_service.dependencies import book_id_var
from multimodal_pipeline_service.models import (
    AudioParseResult,
    DocumentParseResult,
    ExtractedDataField,
    ImageParseResult,
    MultimodalProcessingTaskCreate,
    MultimodalProcessingTaskInDB,
    MultimodalProcessingTaskUpdate,
    UserCorrection,
    UserCorrectionInDB,
)
from neo4j import AsyncSession
from pydantic import BaseModel


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound.

    ``book_id`` comes from the request-scoped X-Book-ID header (verified by
    the gateway); it is None for personal/unscoped calls.
    """
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


# Helper function to convert Pydantic models to Neo4j-compatible dictionary (handles nested models)
def _to_neo4j_props(model_instance: BaseModel) -> Dict[str, Any]:
    data = model_instance.model_dump()
    # Convert nested Pydantic models to JSON strings for Neo4j storage
    for key, value in data.items():
        if isinstance(value, (DocumentParseResult, AudioParseResult, ImageParseResult)):
            data[key] = json.dumps(value.model_dump())
        elif isinstance(value, list) and all(isinstance(item, ExtractedDataField) for item in value):
            data[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, datetime):
            data[key] = value.isoformat()
    return data


# Helper function to reconstruct Pydantic models from Neo4j properties
def _from_neo4j_props(node_props: Dict[str, Any], model_class: BaseModel) -> BaseModel:
    props = node_props.copy()
    props.pop("book_id", None)  # Book scoping marker, not part of the API models
    if "created_at" in props and isinstance(props["created_at"], str):
        props["created_at"] = datetime.fromisoformat(props["created_at"])
    if "updated_at" in props and isinstance(props["updated_at"], str):
        props["updated_at"] = datetime.fromisoformat(props["updated_at"])
    if "processing_start_time" in props and isinstance(props["processing_start_time"], str):
        props["processing_start_time"] = datetime.fromisoformat(props["processing_start_time"])
    if "processing_end_time" in props and isinstance(props["processing_end_time"], str):
        props["processing_end_time"] = datetime.fromisoformat(props["processing_end_time"])
    if "last_review_request_time" in props and isinstance(props["last_review_request_time"], str):
        props["last_review_request_time"] = datetime.fromisoformat(props["last_review_request_time"])
    if "submitted_at" in props and isinstance(props["submitted_at"], str):
        props["submitted_at"] = datetime.fromisoformat(props["submitted_at"])

    # Reconstruct nested Pydantic models from JSON strings
    if "document_result" in props and isinstance(props["document_result"], str):
        props["document_result"] = DocumentParseResult(**json.loads(props["document_result"]))
    if "audio_result" in props and isinstance(props["audio_result"], str):
        props["audio_result"] = AudioParseResult(**json.loads(props["audio_result"]))
    if "image_result" in props and isinstance(props["image_result"], str):
        props["image_result"] = ImageParseResult(**json.loads(props["image_result"]))
    if "extracted_data" in props and isinstance(props["extracted_data"], str):
        props["extracted_data"] = [ExtractedDataField(**item) for item in json.loads(props["extracted_data"])]
    if "extracted_commands" in props and isinstance(props["extracted_commands"], str):
        props["extracted_commands"] = [ExtractedDataField(**item) for item in json.loads(props["extracted_commands"])]
    if "extracted_objects" in props and isinstance(props["extracted_objects"], str):
        props["extracted_objects"] = [ExtractedDataField(**item) for item in json.loads(props["extracted_objects"])]

    return model_class(**props)


# --- MultimodalProcessingTask CRUD ---
async def create_multimodal_processing_task(
    session: AsyncSession, task_data: MultimodalProcessingTaskCreate
) -> MultimodalProcessingTaskInDB:
    task_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(task_data)
    props["id"] = task_id
    props["book_id"] = book_id_var.get()
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (mpt:MultimodalProcessingTask $props)
    CREATE (u)-[:OWNS_MULTIMODAL_TASK]->(mpt)
    RETURN mpt
    """
    result = await session.run(query, user_id=task_data.user_id, props=props)
    record = await result.single()

    return _from_neo4j_props(record["mpt"], MultimodalProcessingTaskInDB)


async def get_multimodal_processing_task(session: AsyncSession, task_id: str) -> Optional[MultimodalProcessingTaskInDB]:
    query = """
    MATCH (mpt:MultimodalProcessingTask {id: $task_id})
    WHERE $book_id IS NULL OR mpt.book_id = $book_id
    RETURN mpt
    """
    result = await _run(session, query, task_id=task_id)
    record = await result.single()

    if record:
        return _from_neo4j_props(record["mpt"], MultimodalProcessingTaskInDB)
    return None


async def get_all_multimodal_processing_tasks(
    session: AsyncSession, user_id: str
) -> List[MultimodalProcessingTaskInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_MULTIMODAL_TASK]->(mpt:MultimodalProcessingTask)
    WHERE $book_id IS NULL OR mpt.book_id = $book_id
    RETURN mpt
    ORDER BY mpt.created_at DESC
    """
    result = await _run(session, query, user_id=user_id)
    tasks = []
    async for record in result:
        tasks.append(_from_neo4j_props(record["mpt"], MultimodalProcessingTaskInDB))
    return tasks


async def update_multimodal_processing_task(
    session: AsyncSession, task_id: str, task_data: MultimodalProcessingTaskUpdate
) -> Optional[MultimodalProcessingTaskInDB]:
    update_fields = task_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_multimodal_processing_task(session, task_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    # Convert nested models to JSON string before update
    for key, value in update_fields.items():
        if isinstance(value, (DocumentParseResult, AudioParseResult, ImageParseResult)):
            update_fields[key] = json.dumps(value.model_dump())
        elif isinstance(value, list) and all(isinstance(item, ExtractedDataField) for item in value):
            update_fields[key] = json.dumps([item.model_dump() for item in value])
        elif isinstance(value, datetime):
            update_fields[key] = value.isoformat()

    set_clauses = [f"mpt.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (mpt:MultimodalProcessingTask {{id: $task_id}})
    WHERE $book_id IS NULL OR mpt.book_id = $book_id
    SET {set_query_part}
    RETURN mpt
    """
    params = {"task_id": task_id, **update_fields}
    result = await _run(session, query, params)
    record = await result.single()

    if record:
        return _from_neo4j_props(record["mpt"], MultimodalProcessingTaskInDB)
    return None


async def delete_multimodal_processing_task(session: AsyncSession, task_id: str) -> bool:
    query = """
    MATCH (mpt:MultimodalProcessingTask {id: $task_id})
    WHERE $book_id IS NULL OR mpt.book_id = $book_id
    DETACH DELETE mpt
    """
    result = await _run(session, query, task_id=task_id)
    return result.consume().counters.nodes_deleted > 0


# --- UserCorrection CRUD ---
async def create_user_correction(session: AsyncSession, correction_data: UserCorrection) -> UserCorrectionInDB:
    correction_id = str(uuid.uuid4())
    submitted_at = datetime.now(timezone.utc)

    props = correction_data.model_dump()
    props["id"] = correction_id
    props["book_id"] = book_id_var.get()
    props["submitted_at"] = submitted_at.isoformat()

    query = """
    MATCH (mpt:MultimodalProcessingTask {id: $task_id})
    WHERE $book_id IS NULL OR mpt.book_id = $book_id
    CREATE (uc:UserCorrection $props)
    CREATE (mpt)-[:HAS_CORRECTION]->(uc)
    RETURN uc
    """
    result = await _run(session, query, task_id=correction_data.task_id, props=props)
    record = await result.single()

    return _from_neo4j_props(record["uc"], UserCorrectionInDB)


async def get_user_corrections_for_task(session: AsyncSession, task_id: str) -> List[UserCorrectionInDB]:
    query = """
    MATCH (mpt:MultimodalProcessingTask {id: $task_id})-[:HAS_CORRECTION]->(uc:UserCorrection)
    WHERE $book_id IS NULL OR mpt.book_id = $book_id
    RETURN uc
    ORDER BY uc.submitted_at DESC
    """
    result = await _run(session, query, task_id=task_id)
    corrections = []
    async for record in result:
        corrections.append(_from_neo4j_props(record["uc"], UserCorrectionInDB))
    return corrections
