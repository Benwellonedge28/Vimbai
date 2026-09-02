"""
Related Party Service
Port: 8204
Related party identification and disclosure
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Related Party Service", version="1.0.0")


class RelatedParty(BaseModel):
    party_id: str
    relationship: str
    party_name: str
    transactions_count: int
    total_amount: float
    nature_of_transactions: List[str]


class RelatedPartyRequest(BaseModel):
    company_id: str
    audit_id: str
    management_compensation: float
    identified_parties: List[Dict[str, Any]]
    transactions: List[Dict[str, Any]]
    arms_length_comparison: bool


class RelatedPartyResponse(BaseModel):
    company_id: str
    audit_id: str
    related_parties: List[RelatedParty]
    total_related_party_revenue: float
    total_related_party_expenses: float
    significant_transactions: List[Dict[str, Any]]
    arms_length_conclusion: str
    disclosure_completeness: str
    recommendations: List[str]


async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "related-party", "version": "1.0.0"}


@app.post("/analyze", response_model=RelatedPartyResponse)
async def analyze_related_parties(request: RelatedPartyRequest):
    logger.info("Analyzing related parties", company=request.company_id, audit=request.audit_id)

    parties_dict = {}
    for party in request.identified_parties:
        party_id = party.get("id", "")
        parties_dict[party_id] = RelatedParty(
            party_id=party_id,
            relationship=party.get("relationship", ""),
            party_name=party.get("name", ""),
            transactions_count=0,
            total_amount=0.0,
            nature_of_transactions=[],
        )

    significant_transactions = []
    for txn in request.transactions:
        party_id = txn.get("related_party_id", "")
        if party_id in parties_dict:
            parties_dict[party_id].transactions_count += 1
            parties_dict[party_id].total_amount += txn.get("amount", 0)
            nature = txn.get("nature", "")
            if nature and nature not in parties_dict[party_id].nature_of_transactions:
                parties_dict[party_id].nature_of_transactions.append(nature)

            if txn.get("amount", 0) > 1000000:
                significant_transactions.append(
                    {"party": party_id, "amount": txn.get("amount", 0), "nature": nature, "terms": txn.get("terms", "")}
                )

    total_revenue = sum(
        p.total_amount
        for p in parties_dict.values()
        if "subsidiary" in p.relationship.lower() or "associate" in p.relationship.lower()
    )
    total_expenses = sum(p.total_amount for p in parties_dict.values() if "management" in p.relationship.lower())

    arms_length = "Transfers at arm's length" if request.arms_length_comparison else "Review of terms required"
    disclosure = "Complete" if len(parties_dict) > 0 else "Incomplete"

    return RelatedPartyResponse(
        company_id=request.company_id,
        audit_id=request.audit_id,
        related_parties=list(parties_dict.values()),
        total_related_party_revenue=round(total_revenue, 2),
        total_related_party_expenses=round(total_expenses + request.management_compensation, 2),
        significant_transactions=(
            significant_transactions
            if significant_transactions
            else [{"message": "No individually significant transactions"}]
        ),
        arms_length_conclusion=arms_length,
        disclosure_completeness=disclosure,
        recommendations=[
            "Ensure all related party relationships are disclosed",
            "Document arm's length nature of transactions",
            "Review and approve related party transactions by independent directors",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8204)
