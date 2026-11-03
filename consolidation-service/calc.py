"""Consolidation: parent and subsidiaries into one set of statements.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("consolidation_json",)

# The parent's own figures are still placeholders, as they were in the endpoint:
# in production they would come from the accounting service for the same period.
PARENT_REVENUE = 10_000_000.0
PARENT_PROFIT = 1_500_000.0
PARENT_ASSETS = 50_000_000.0
PARENT_LIABILITIES = 30_000_000.0


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    """The extension, but only when it carries the functions this service needs."""
    try:
        import vimbai_native
    except ImportError:
        return None
    if not all(hasattr(vimbai_native, name) for name in REQUIRED):
        return None
    return vimbai_native


def engine_in_use() -> str:
    """Announce the calculator so /health can be read in production."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    if requested in ("python", "reference"):
        return "python"
    if _extension() is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is missing or too old")
        return "python"
    return "rust"


def _call(name, payload):
    """Ask the extension for one figure set, or None when the reference must answer."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    if requested in ("python", "reference"):
        return None
    native = _extension()
    if native is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is missing or too old")
        return None
    return json.loads(getattr(native, name)(json.dumps(payload)))


def consolidate(parent_company_id, period, subsidiaries, intercompany_transactions, minority_interest):
    """One set of statements from a parent and its subsidiaries.

    Revenue and profit come in by ownership share, while assets and liabilities come
    in whole; that asymmetry is what leaves the non-controlling interest holding a
    slice of equity it never contributed revenue for. Intercompany sales come out of
    revenue, and a subsidiary in loss reduces the group's profit through its share.
    """
    payload = {
        "parent_company_id": parent_company_id,
        "period": period,
        "parent_revenue": PARENT_REVENUE,
        "parent_profit": PARENT_PROFIT,
        "parent_assets": PARENT_ASSETS,
        "parent_liabilities": PARENT_LIABILITIES,
        "subsidiaries": [
            {
                "subsidiary_id": s["subsidiary_id"],
                "name": s.get("subsidiary_name", ""),
                "ownership_percentage": s["ownership_percentage"],
                "non_controlling_interests_percentage": s.get("non_controlling_interests_percentage"),
                "revenue": s.get("financial_data", {}).get("revenue", 2_000_000.0),
                "profit": s.get("financial_data", {}).get("profit", 300_000.0),
                "assets": s.get("financial_data", {}).get("assets", 5_000_000.0),
                "liabilities": s.get("financial_data", {}).get("liabilities", 2_500_000.0),
                "equity": s.get("financial_data", {}).get("equity"),
                "goodwill": s.get("goodwill", 0.0),
            }
            for s in subsidiaries
        ],
        "intercompany_transactions": [
            {"transaction_type": t["transaction_type"], "amount": t["amount"]} for t in intercompany_transactions
        ],
    }
    core = _call("consolidation_json", payload)
    if core is None:
        core = _reference(payload)
    return _present(core, payload, minority_interest)


def _present(core, payload, minority_interest):
    """Wrap the core figures in the shape this endpoint has always returned.

    The split of assets and liabilities, and the cash flow block below, are the
    endpoint's own presentation rather than anything the library measures. The cash
    flow figures in particular are fixed placeholders inherited from the original
    endpoint: they describe no real statement, and they are called out here so nobody
    mistakes them for a calculation.
    """
    sheet = core["consolidated_balance_sheet"]
    total_assets = sheet["total_assets"]
    total_liabilities = sheet["total_liabilities"]

    total_sub_equity = 0.0
    for sub in payload["subsidiaries"]:
        total_sub_equity += sub["equity"] if sub["equity"] is not None else sub["assets"] - sub["liabilities"]
    nci_percentage = (
        sum(
            (
                s["non_controlling_interests_percentage"]
                if s["non_controlling_interests_percentage"] is not None
                else 100 - s["ownership_percentage"]
            )
            for s in payload["subsidiaries"]
        )
        / len(payload["subsidiaries"])
        if payload["subsidiaries"]
        else 0.0
    )
    nci_equity_share = total_sub_equity * (nci_percentage / 100)
    minority = (total_sub_equity + PARENT_ASSETS - PARENT_LIABILITIES) * 0.1 if minority_interest else 0.0

    core["consolidated_profit"]["goodwill_amortization"] = 50_000.0
    sheet["non_current_assets"] = round(total_assets * 0.6, 2)
    sheet["current_assets"] = round(total_assets * 0.4, 2)
    sheet["non_current_liabilities"] = round(total_liabilities * 0.4, 2)
    sheet["current_liabilities"] = round(total_liabilities * 0.6, 2)
    core["consolidated_cash_flow"] = {
        "operating_activities": 2_000_000.0,
        "investing_activities": -800_000.0,
        "financing_activities": -500_000.0,
        "effect_of_exchange_rate": 10_000.0,
        "net_increase_in_cash": 710_000.0,
    }
    core["nci_at_fair_value"] = round(nci_equity_share * 1.2, 2)
    core["minority_interest"] = round(minority, 2)
    return core


def _reference(payload):
    """The original Python consolidation, kept as the oracle."""
    subsidiary_revenues = {}
    subsidiary_profits = {}
    total_sub_revenue = total_sub_profit = 0.0
    total_sub_assets = total_sub_liabilities = total_sub_equity = 0.0
    for sub in payload["subsidiaries"]:
        ownership = sub["ownership_percentage"] / 100
        sub_assets = sub["assets"]
        sub_liabilities = sub["liabilities"]
        equity = sub["equity"] if sub["equity"] is not None else sub_assets - sub_liabilities
        subsidiary_revenues[sub["subsidiary_id"]] = round(sub["revenue"] * ownership, 2)
        subsidiary_profits[sub["subsidiary_id"]] = round(sub["profit"] * ownership, 2)
        total_sub_revenue += sub["revenue"] * ownership
        total_sub_profit += sub["profit"] * ownership
        total_sub_assets += sub_assets
        total_sub_liabilities += sub_liabilities
        total_sub_equity += equity

    intercompany_sales = sum(
        t["amount"] for t in payload["intercompany_transactions"] if t["transaction_type"] == "sale"
    )
    nci_pct_avg = (
        sum(
            (
                s["non_controlling_interests_percentage"]
                if s["non_controlling_interests_percentage"] is not None
                else 100 - s["ownership_percentage"]
            )
            for s in payload["subsidiaries"]
        )
        / len(payload["subsidiaries"])
        if payload["subsidiaries"]
        else 0.0
    )
    nci_profit_share = total_sub_profit * (nci_pct_avg / 100)
    nci_equity_share = total_sub_equity * (nci_pct_avg / 100)

    consolidated_revenue = PARENT_REVENUE + total_sub_revenue - intercompany_sales
    consolidated_profit = PARENT_PROFIT + total_sub_profit - nci_profit_share
    total_assets = PARENT_ASSETS + total_sub_assets
    total_liabilities = PARENT_LIABILITIES + total_sub_liabilities
    total_equity = total_assets - total_liabilities

    return {
        "parent_company_id": payload["parent_company_id"],
        "period": payload["period"],
        "consolidated_revenue": {
            "parent_revenue": PARENT_REVENUE,
            "subsidiary_revenues": subsidiary_revenues,
            "intercompany_sales_elimination": round(intercompany_sales, 2),
            "consolidated_revenue": round(consolidated_revenue, 2),
        },
        "consolidated_profit": {
            "parent_profit": PARENT_PROFIT,
            "subsidiary_profits": subsidiary_profits,
            "nci_share": round(nci_profit_share, 2),
            "attributable_to_parent": round(consolidated_profit, 2),
        },
        "consolidated_balance_sheet": {
            "total_assets": round(total_assets, 2),
            "total_liabilities": round(total_liabilities, 2),
            "equity_attributable_to_parent": round(total_equity - nci_equity_share, 2),
            "non_controlling_interests": round(nci_equity_share, 2),
            "total_equity": round(total_equity, 2),
        },
        "goodwill_recognized": sum(s["goodwill"] for s in payload["subsidiaries"]),
        "elimination_count": len(payload["intercompany_transactions"]),
    }
