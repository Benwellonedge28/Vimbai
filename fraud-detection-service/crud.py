import uuid
from datetime import datetime, timezone
from typing import List, Literal, Optional

from fraud_detection_service.models import FraudulentTransactionFlagCreate, FraudulentTransactionFlagInDB
from neo4j import AsyncSession


async def create_fraud_flag(
    session: AsyncSession, flag_data: FraudulentTransactionFlagCreate
) -> FraudulentTransactionFlagInDB:
    flag_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    query = """
    CREATE (f:FraudulentTransactionFlag {
        id: $id,
        transaction_id: $transaction_id,
        fraud_score: toFloat($fraud_score),
        fraud_flag: $fraud_flag,
        reason: $reason,
        model_version: $model_version,
        flagged_by_user_id: $flagged_by_user_id,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    RETURN f
    """
    params = flag_data.model_dump()
    params["id"] = flag_neo4j_id
    params["fraud_score"] = float(params["fraud_score"])  # Neo4j stores float, not Decimal
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    flag_node = record["f"]

    return FraudulentTransactionFlagInDB(
        id=flag_node["id"],
        transaction_id=flag_node["transaction_id"],
        fraud_score=flag_node["fraud_score"],
        fraud_flag=flag_node["fraud_flag"],
        reason=flag_node["reason"],
        model_version=flag_node["model_version"],
        flagged_by_user_id=flag_node["flagged_by_user_id"],
        status=flag_node["status"],
        created_at=datetime.fromisoformat(flag_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(flag_node["updated_at"].iso_format()),
    )


async def get_fraud_flag(session: AsyncSession, flag_id: str) -> Optional[FraudulentTransactionFlagInDB]:
    query = """
    MATCH (f:FraudulentTransactionFlag {id: $flag_id})
    RETURN f
    """
    result = await session.run(query, flag_id=flag_id)
    record = await result.single()

    if record:
        flag_node = record["f"]
        return FraudulentTransactionFlagInDB(
            id=flag_node["id"],
            transaction_id=flag_node["transaction_id"],
            fraud_score=flag_node["fraud_score"],
            fraud_flag=flag_node["fraud_flag"],
            reason=flag_node["reason"],
            model_version=flag_node["model_version"],
            flagged_by_user_id=flag_node["flagged_by_user_id"],
            status=flag_node["status"],
            created_at=datetime.fromisoformat(flag_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(flag_node["updated_at"].iso_format()),
        )
    return None


async def get_all_fraud_flags(session: AsyncSession) -> List[FraudulentTransactionFlagInDB]:
    query = """
    MATCH (f:FraudulentTransactionFlag)
    RETURN f
    ORDER BY f.created_at DESC
    """
    result = await session.run(query)
    flags = []
    async for record in result:
        flag_node = record["f"]
        flags.append(
            FraudulentTransactionFlagInDB(
                id=flag_node["id"],
                transaction_id=flag_node["transaction_id"],
                fraud_score=flag_node["fraud_score"],
                fraud_flag=flag_node["fraud_flag"],
                reason=flag_node["reason"],
                model_version=flag_node["model_version"],
                flagged_by_user_id=flag_node["flagged_by_user_id"],
                status=flag_node["status"],
                created_at=datetime.fromisoformat(flag_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(flag_node["updated_at"].iso_format()),
            )
        )
    return flags


async def update_fraud_flag_status(
    session: AsyncSession,
    flag_id: str,
    new_status: Literal["open", "investigating", "false_positive", "confirmed_fraud"],
) -> Optional[FraudulentTransactionFlagInDB]:
    updated_at = datetime.now(timezone.utc).isoformat()
    query = """
    MATCH (f:FraudulentTransactionFlag {id: $flag_id})
    SET f.status = $new_status, f.updated_at = datetime($updated_at)
    RETURN f
    """
    params = {"flag_id": flag_id, "new_status": new_status, "updated_at": updated_at}
    result = await session.run(query, params)
    record = await result.single()
    if record:
        return await get_fraud_flag(session, flag_id)
    return None
