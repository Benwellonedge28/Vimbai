"""
Debt Management Service CRUD Operations

Neo4j-backed persistence for loans. All records are stamped with
book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.

Amortization schedules are pure computation (no storage), but the loan
they are generated from must belong to the caller and be Book-visible,
otherwise 404. The debt summary is computed from the caller's visible
loans for the company.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from debt_management_service.dependencies import book_id_var
from debt_management_service.exceptions import NotFoundError
from debt_management_service.models import (
    AmortizationScheduleItem,
    DebtSummary,
    Loan,
    LoanCreate,
)
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


def _d(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    return datetime.fromisoformat(value.iso_format())


def _loan_from_node(n: Dict[str, Any], user_id: str) -> Loan:
    return Loan(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        loan_name=n["loan_name"],
        lender=n["lender"],
        principal=float(n.get("principal", 0)),
        interest_rate=float(n.get("interest_rate", 0)),
        term_months=int(n.get("term_months", 0)),
        disbursement_date=n["disbursement_date"],
        payment_frequency=n.get("payment_frequency", "monthly"),
        remaining_balance=float(n.get("remaining_balance", 0)),
        status=n.get("status", "active"),
        created_at=_d(n.get("created_at")),
    )


def calc_payment(principal: float, rate: float, months: int) -> float:
    if rate == 0:
        return principal / months
    r = rate / 12
    return principal * r * (1 + r) ** months / ((1 + r) ** months - 1)


async def create_loan(session: AsyncSession, user_id: str, payload: LoanCreate) -> Loan:
    loan_id = str(uuid.uuid4())
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Loan {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        loan_name: $loan_name,
        lender: $lender,
        principal: toFloat($principal),
        interest_rate: toFloat($interest_rate),
        term_months: toInteger($term_months),
        disbursement_date: $disbursement_date,
        payment_frequency: $payment_frequency,
        remaining_balance: toFloat($principal),
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_LOAN]->(x)
    RETURN x
    """
    params = {
        "id": loan_id,
        "user_id": user_id,
        "company_id": payload.company_id,
        "loan_name": payload.loan_name,
        "lender": payload.lender,
        "principal": payload.principal,
        "interest_rate": payload.interest_rate,
        "term_months": payload.term_months,
        "disbursement_date": payload.disbursement_date.isoformat(),
        "payment_frequency": payload.payment_frequency,
        "status": payload.status,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _loan_from_node(dict(records[0]["x"]), user_id)


async def list_loans(session: AsyncSession, user_id: str, company_id: str) -> List[Loan]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_LOAN]->(x:Loan {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_loan_from_node(dict(r["x"]), user_id) async for r in result]


async def get_loan(session: AsyncSession, user_id: str, company_id: str, loan_id: str) -> Loan:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_LOAN]->(x:Loan {{company_id: $company_id, id: $loan_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id, loan_id=loan_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Loan not found")
    return _loan_from_node(dict(records[0]["x"]), user_id)


def build_schedule(loan: Loan) -> List[AmortizationScheduleItem]:
    payment = calc_payment(loan.principal, loan.interest_rate, loan.term_months)
    balance = loan.principal
    schedule = []
    for i in range(1, loan.term_months + 1):
        interest = balance * (loan.interest_rate / 12)
        principal_comp = payment - interest
        balance -= principal_comp
        schedule.append(
            AmortizationScheduleItem(
                period=i,
                payment=round(payment, 2),
                principal_component=round(principal_comp, 2),
                interest_component=round(interest, 2),
                balance=round(max(balance, 0), 2),
            )
        )
    return schedule


async def get_debt_summary(session: AsyncSession, user_id: str, company_id: str, equity: float) -> DebtSummary:
    loans = await list_loans(session, user_id, company_id)
    total_debt = sum(l.remaining_balance for l in loans)
    total_monthly = sum(calc_payment(l.principal, l.interest_rate, l.term_months) for l in loans)
    total_interest = sum(
        calc_payment(l.principal, l.interest_rate, l.term_months) * l.term_months - l.principal for l in loans
    )
    weighted_rate = sum(l.interest_rate * l.remaining_balance for l in loans) / total_debt if total_debt else 0
    d2e = total_debt / equity if equity else 0

    return DebtSummary(
        company_id=company_id,
        total_debt=round(total_debt, 2),
        total_interest=round(total_interest, 2),
        total_monthly_payments=round(total_monthly, 2),
        debt_to_equity=round(d2e, 4),
        weighted_avg_rate=round(weighted_rate, 4),
        loans=[
            {
                "id": l.id,
                "name": l.loan_name,
                "balance": l.remaining_balance,
                "rate": l.interest_rate,
                "status": l.status,
            }
            for l in loans
        ],
    )
