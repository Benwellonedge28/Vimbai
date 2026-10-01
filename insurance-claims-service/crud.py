"""
Insurance Claims Service CRUD Operations

Neo4j-backed persistence for insurance claims. All records are stamped
with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from insurance_claims_service.dependencies import book_id_var
from insurance_claims_service.exceptions import NotFoundError
from insurance_claims_service.models import ClaimResult, ClaimStatus, InsuranceClaim, InsuranceClaimCreate
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _claim_from_node(n: Dict, user_id: str) -> InsuranceClaim:
    return InsuranceClaim(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        policy_number=n["policy_number"],
        claim_type=n.get("claim_type", ""),
        incident_date=n.get("incident_date", ""),
        claim_date=n.get("claim_date", ""),
        claim_amount=float(n.get("claim_amount", 0)),
        deductible=float(n.get("deductible", 0)),
        description=n.get("description", ""),
        supporting_docs=json.loads(n.get("supporting_docs_json") or "[]"),
        coverage_limit=float(n.get("coverage_limit", 0)),
        status=n.get("status", "filed"),
    )


async def file_claim(session: AsyncSession, user_id: str, payload: InsuranceClaimCreate) -> InsuranceClaim:
    claim_id = str(uuid.uuid4())
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:InsuranceClaim {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        policy_number: $policy_number,
        claim_type: $claim_type,
        incident_date: $incident_date,
        claim_date: $claim_date,
        claim_amount: toFloat($claim_amount),
        deductible: toFloat($deductible),
        description: $description,
        supporting_docs_json: $supporting_docs_json,
        coverage_limit: toFloat($coverage_limit),
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CLAIM]->(x)
    RETURN x
    """
    params = {
        "id": claim_id,
        "user_id": user_id,
        "company_id": payload.company_id,
        "policy_number": payload.policy_number,
        "claim_type": payload.claim_type,
        "incident_date": payload.incident_date,
        "claim_date": _now().strftime("%Y-%m-%d"),
        "claim_amount": payload.claim_amount,
        "deductible": payload.deductible,
        "description": payload.description,
        "supporting_docs_json": json.dumps(list(payload.supporting_docs)),
        "coverage_limit": payload.coverage_limit,
        "status": ClaimStatus.FILED.value,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _claim_from_node(dict(records[0]["x"]), user_id)


async def list_claims(session: AsyncSession, user_id: str, company_id: str, status: str = "") -> List[InsuranceClaim]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CLAIM]->(x:InsuranceClaim {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    claims = [_claim_from_node(dict(r["x"]), user_id) async for r in result]
    if status:
        claims = [c for c in claims if c.status.value == status]
    return claims


async def _get_claim(session: AsyncSession, user_id: str, company_id: str, claim_id: str) -> Optional[InsuranceClaim]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CLAIM]->(x:InsuranceClaim {{id: $claim_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, claim_id=claim_id)
    records = [r async for r in result]
    if not records:
        return None
    claim = _claim_from_node(dict(records[0]["x"]), user_id)
    if claim.company_id != company_id:
        return None
    return claim


async def _write_back_status(session: AsyncSession, claim: InsuranceClaim):
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CLAIM]->(x:InsuranceClaim {{id: $claim_id}})
    {BOOK_FILTER}
    SET x.status = $status
    RETURN x
    """
    params = {"user_id": claim.user_id, "claim_id": claim.id, "status": claim.status.value}
    await _run(session, query, params)


async def process_claim(session: AsyncSession, user_id: str, company_id: str, claim_id: str) -> ClaimResult:
    claim = await _get_claim(session, user_id, company_id, claim_id)
    if not claim:
        raise NotFoundError("Claim not found")

    claim.status = ClaimStatus.UNDER_REVIEW

    covered = claim.claim_amount
    if claim.coverage_limit > 0:
        covered = min(covered, claim.coverage_limit)
    covered -= claim.deductible
    covered = max(covered, 0)

    coverage_ratio = covered / claim.claim_amount if claim.claim_amount else 0

    claim.status = ClaimStatus.APPROVED if covered > 0 else ClaimStatus.DENIED
    await _write_back_status(session, claim)

    return ClaimResult(
        claim_id=claim.id,
        company_id=company_id,
        status=claim.status,
        covered_amount=round(covered, 2),
        deductible_applied=claim.deductible,
        settlement_amount=round(covered, 2),
        coverage_ratio=round(coverage_ratio, 4),
    )
