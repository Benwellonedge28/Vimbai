"""
Financial Identity Service CRUD Operations

KYC profiles move from an in-memory _profiles dict to Neo4j:
:FinancialProfile nodes via :OWNS_PROFILE edges, book_id stamped,
Book-gated reads. The kyc_documents list is stored as a JSON prop.

Note the two distinct identities: the profile's user_id field names the
KYC *subject* (who the profile describes, stored on the node under the distinct
subject_id prop), while record ownership is the
caller's X-User-Id - profiles are owned by whoever created them and
only readable within that owner's scope + Book.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, Optional

from financial_identity_service.dependencies import book_id_var
from financial_identity_service.exceptions import NotFoundError
from financial_identity_service.models import FinancialProfile, FinancialProfileCreate, VerificationStatus
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


def _as_dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _profile_from_node(n: Dict[str, Any]) -> FinancialProfile:
    return FinancialProfile(
        id=n["id"],
        user_id=n["subject_id"],
        legal_name=n["legal_name"],
        national_id=n.get("national_id", ""),
        tax_id=n.get("tax_id", ""),
        date_of_birth=_as_dt(n.get("date_of_birth")),
        address=n.get("address", ""),
        phone=n.get("phone", ""),
        email=n.get("email", ""),
        employer=n.get("employer", ""),
        annual_income=float(n.get("annual_income", 0)),
        verification_status=n.get("verification_status", "pending"),
        verified_at=_as_dt(n.get("verified_at")),
        risk_score=int(n.get("risk_score", 0)),
        kyc_documents=json.loads(n.get("kyc_documents_json") or "[]"),
    )


async def create_profile(session: AsyncSession, user_id: str, payload: FinancialProfileCreate) -> FinancialProfile:
    profile = FinancialProfile(
        id=str(uuid.uuid4()),
        user_id=payload.user_id,
        legal_name=payload.legal_name,
        national_id=payload.national_id,
        tax_id=payload.tax_id,
        date_of_birth=payload.date_of_birth,
        address=payload.address,
        phone=payload.phone,
        email=payload.email,
        employer=payload.employer,
        annual_income=payload.annual_income,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:FinancialProfile {
        id: $id,
        book_id: $book_id,
        subject_id: $subject_id,
        legal_name: $legal_name,
        national_id: $national_id,
        tax_id: $tax_id,
        date_of_birth: datetime($date_of_birth),
        address: $address,
        phone: $phone,
        email: $email,
        employer: $employer,
        annual_income: toFloat($annual_income),
        verification_status: $verification_status,
        verified_at: datetime($verified_at),
        risk_score: toInteger($risk_score),
        kyc_documents_json: $kyc_documents_json,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PROFILE]->(x)
    RETURN x
    """
    params = {
        "id": profile.id,
        "user_id": user_id,
        "subject_id": profile.user_id,
        "legal_name": profile.legal_name,
        "national_id": profile.national_id,
        "tax_id": profile.tax_id,
        "date_of_birth": profile.date_of_birth.isoformat() if profile.date_of_birth else None,
        "address": profile.address,
        "phone": profile.phone,
        "email": profile.email,
        "employer": profile.employer,
        "annual_income": profile.annual_income,
        "verification_status": profile.verification_status.value,
        "verified_at": None,
        "risk_score": profile.risk_score,
        "kyc_documents_json": "[]",
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return profile


async def get_profile(session: AsyncSession, user_id: str, profile_id: str) -> FinancialProfile:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PROFILE]->(x:FinancialProfile {{id: $profile_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, profile_id=profile_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Profile not found")
    return _profile_from_node(dict(records[0]["x"]))


async def verify_profile(session: AsyncSession, user_id: str, profile_id: str, documents: list) -> Dict[str, Any]:
    profile = await get_profile(session, user_id, profile_id)
    profile.kyc_documents = list(documents)
    if len(documents) >= 2:
        profile.verification_status = VerificationStatus.VERIFIED
        profile.verified_at = _now()
        profile.risk_score = 20  # low risk
    else:
        profile.risk_score = 80  # high risk
    query = """
    MATCH (x:FinancialProfile {id: $id})
    SET x.kyc_documents_json = $kyc_documents_json,
        x.verification_status = $verification_status,
        x.verified_at = datetime($verified_at),
        x.risk_score = toInteger($risk_score)
    """
    params = {
        "id": profile.id,
        "kyc_documents_json": json.dumps(profile.kyc_documents),
        "verification_status": profile.verification_status.value,
        "verified_at": profile.verified_at.isoformat() if profile.verified_at else None,
        "risk_score": profile.risk_score,
    }
    await _run(session, query, params)
    return {"id": profile_id, "status": profile.verification_status.value, "risk_score": profile.risk_score}


async def get_by_user(session: AsyncSession, user_id: str, subject_id: str) -> FinancialProfile:
    """Find the caller's Book-visible profile for a given KYC subject."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PROFILE]->(x:FinancialProfile {{subject_id: $subject_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    LIMIT 1
    """
    result = await _run(session, query, user_id=user_id, subject_id=subject_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("No profile for user")
    return _profile_from_node(dict(records[0]["x"]))
