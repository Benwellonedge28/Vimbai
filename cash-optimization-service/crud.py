"""
Cash Optimization Service CRUD Operations

Neo4j-backed persistence for cash accounts and optimization
suggestions. All records are stamped with book_id; every read applies
the Book filter `WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List

from cash_optimization_service.dependencies import book_id_var
from cash_optimization_service.exceptions import NotFoundError
from cash_optimization_service.models import (
    AccountType,
    CashAccount,
    CashAccountCreate,
    OptimizationSuggestion,
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


def _account_from_node(n: Dict[str, Any], user_id: str) -> CashAccount:
    return CashAccount(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        account_name=n["account_name"],
        account_type=n.get("account_type", "operating"),
        balance=float(n.get("balance", 0)),
        min_required=float(n.get("min_required", 0)),
        interest_rate=float(n.get("interest_rate", 0)),
        currency=n.get("currency", "USD"),
    )


def _suggestion_from_node(n: Dict[str, Any], user_id: str) -> OptimizationSuggestion:
    return OptimizationSuggestion(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        from_account=n["from_account"],
        to_account=n["to_account"],
        amount=float(n.get("amount", 0)),
        reason=n["reason"],
        expected_benefit=float(n.get("expected_benefit", 0)),
        priority=n.get("priority", "medium"),
    )


async def _list_accounts(session: AsyncSession, user_id: str, company_id: str) -> List[CashAccount]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ACCOUNT]->(x:CashAccount {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_account_from_node(dict(r["x"]), user_id) async for r in result]


async def add_account(session: AsyncSession, user_id: str, payload: CashAccountCreate) -> Dict[str, Any]:
    account = CashAccount(
        id=str(uuid.uuid4()),
        user_id=user_id,
        company_id=payload.company_id,
        account_name=payload.account_name,
        account_type=payload.account_type,
        balance=payload.balance,
        min_required=payload.min_required,
        interest_rate=payload.interest_rate,
        currency=payload.currency,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CashAccount {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        account_name: $account_name,
        account_type: $account_type,
        balance: toFloat($balance),
        min_required: toFloat($min_required),
        interest_rate: toFloat($interest_rate),
        currency: $currency,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ACCOUNT]->(x)
    RETURN x
    """
    params = {
        "id": account.id,
        "user_id": user_id,
        "company_id": account.company_id,
        "account_name": account.account_name,
        "account_type": account.account_type.value,
        "balance": account.balance,
        "min_required": account.min_required,
        "interest_rate": account.interest_rate,
        "currency": account.currency,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return {"id": account.id, "account_name": account.account_name, "balance": account.balance}


async def get_accounts(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    accounts = await _list_accounts(session, user_id, company_id)
    return {"company_id": company_id, "accounts": accounts}


def compute_suggestions(company_id: str, accounts: List[CashAccount]) -> List[OptimizationSuggestion]:
    """Pure computation over the caller-visible account set (original semantics)."""
    suggestions = []
    if not accounts:
        return suggestions

    # Find excess cash in operating accounts
    for acc in accounts:
        excess = acc.balance - acc.min_required
        if excess > 10000 and acc.account_type == AccountType.OPERATING:
            inv_accounts = [a for a in accounts if a.account_type == AccountType.INVESTMENT]
            if inv_accounts:
                best = max(inv_accounts, key=lambda a: a.interest_rate)
                annual_benefit = excess * best.interest_rate
                suggestions.append(
                    OptimizationSuggestion(
                        company_id=company_id,
                        from_account=acc.account_name,
                        to_account=best.account_name,
                        amount=excess,
                        reason=(
                            f"Move excess operating cash to higher-yield investment account "
                            f"({best.interest_rate*100:.1f}% APR)"
                        ),
                        expected_benefit=annual_benefit,
                        priority="high",
                    )
                )

        # Check reserve accounts with excess
        if excess > 50000 and acc.account_type == AccountType.RESERVE:
            inv_accounts = [
                a for a in accounts if a.account_type == AccountType.INVESTMENT and a.interest_rate > acc.interest_rate
            ]
            if inv_accounts:
                best = max(inv_accounts, key=lambda a: a.interest_rate)
                rate_diff = best.interest_rate - acc.interest_rate
                annual_benefit = excess * rate_diff
                suggestions.append(
                    OptimizationSuggestion(
                        company_id=company_id,
                        from_account=acc.account_name,
                        to_account=best.account_name,
                        amount=excess,
                        reason=f"Transfer to higher-yield account for {rate_diff*100:.1f}% rate improvement",
                        expected_benefit=annual_benefit,
                        priority="medium",
                    )
                )

    # Check for underfunded accounts
    for acc in accounts:
        if acc.balance < acc.min_required and acc.account_type in (
            AccountType.RESERVE,
            AccountType.TAX,
            AccountType.PAYROLL,
        ):
            deficit = acc.min_required - acc.balance
            operating = [
                a for a in accounts if a.account_type == AccountType.OPERATING and a.balance > a.min_required + deficit
            ]
            if operating:
                source = max(operating, key=lambda a: a.balance - a.min_required)
                suggestions.append(
                    OptimizationSuggestion(
                        company_id=company_id,
                        from_account=source.account_name,
                        to_account=acc.account_name,
                        amount=deficit,
                        reason=f"Top up {acc.account_type.value} account to meet minimum requirement",
                        expected_benefit=0,
                        priority="high",
                    )
                )

    return suggestions


async def run_optimization(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    accounts = await _list_accounts(session, user_id, company_id)
    suggestions = compute_suggestions(company_id, accounts)

    # Replace the caller's previous suggestions for this company (original
    # semantics: each run overwrites the stored set) - only Book-visible ones.
    delete_query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUGGESTION]->(x:OptimizationSuggestion {{company_id: $company_id}})
    {BOOK_FILTER}
    DETACH DELETE x
    """
    await _run(session, delete_query, user_id=user_id, company_id=company_id)

    for s in suggestions:
        s.user_id = user_id
        query = """
        MATCH (u:User {id: $user_id})
        CREATE (x:OptimizationSuggestion {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            company_id: $company_id,
            from_account: $from_account,
            to_account: $to_account,
            amount: toFloat($amount),
            reason: $reason,
            expected_benefit: toFloat($expected_benefit),
            priority: $priority,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_SUGGESTION]->(x)
        RETURN x
        """
        params = {
            "id": str(uuid.uuid4()),
            "user_id": user_id,
            "company_id": s.company_id,
            "from_account": s.from_account,
            "to_account": s.to_account,
            "amount": s.amount,
            "reason": s.reason,
            "expected_benefit": s.expected_benefit,
            "priority": s.priority,
            "created_at": _now().isoformat(),
        }
        await _run(session, query, params)

    total_benefit = sum(s.expected_benefit for s in suggestions)
    return {
        "company_id": company_id,
        "suggestions": suggestions,
        "total_count": len(suggestions),
        "potential_annual_benefit": total_benefit,
    }


async def get_suggestions(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUGGESTION]->(x:OptimizationSuggestion {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    suggestions = [_suggestion_from_node(dict(r["x"]), user_id) async for r in result]
    return {"company_id": company_id, "suggestions": suggestions}
