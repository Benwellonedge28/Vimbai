"""
MFA Auth Service CRUD Operations

TOTP enrollment secrets move from the in-memory _user_secrets dict
to Neo4j: one :MfaSecret node per user via :OWNS_MFA_SECRET, so
enrollment survives restarts (previously a restart silently locked
out every enrolled user). MFA has no Book dimension - a user's
second factor belongs to them across all Books - so access is gated
by CALLER IDENTITY instead: callers may only enroll, verify and
challenge MFA for themselves. Pending challenges are 5-minute
ephemeral one-time artifacts by design and stay in memory.
"""

import json
from typing import Optional

from neo4j import AsyncSession


async def _run(session, query, params=None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    return await session.run(query, merged)


async def store_secret(session: AsyncSession, user_id: str, secret: str, backup_codes: list) -> None:
    """Persist (or overwrite) the caller's MFA secret; latest-wins like the original."""
    query = """
    MATCH (s:MfaSecret {user_id: $user_id})
    DETACH DELETE s
    """
    await _run(session, query, user_id=user_id)
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:MfaSecret {
        user_id: $user_id,
        secret: $secret,
        backup_codes: $backup_codes
    })
    CREATE (u)-[:OWNS_MFA_SECRET]->(x)
    """
    await _run(session, query, user_id=user_id, secret=secret, backup_codes=json.dumps(backup_codes))


async def get_secret(session: AsyncSession, user_id: str) -> Optional[str]:
    """Return the caller's stored TOTP secret, if enrolled."""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_MFA_SECRET]->(x:MfaSecret)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    records = [r async for r in result]
    if not records:
        return None
    return dict(records[0]["x"])["secret"]
