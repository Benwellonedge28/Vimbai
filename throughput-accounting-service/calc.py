"""Throughput accounting calculations.

The figures come from the Rust library when its extension is installed; the
Python maths stays here as the reference implementation and as the fallback for
images built without the wheel. tests/test_cost_accounting_seam.py holds the two
to the same answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    try:
        import vimbai_native
    except ImportError:
        return None
    return vimbai_native


def engine_in_use() -> str:
    """Announce the calculator so /health can be read in production."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    if requested in ("python", "reference"):
        return "python"
    if _extension() is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is not installed")
        return "python"
    return "rust"


def _call(name, payload):
    native = _extension()
    if native is None:
        return None
    return json.loads(getattr(native, name)(json.dumps(payload)))


def analyze(company_id, operating_expenses, products, available_constraint_minutes=480):
    """Throughput analysis with the greedy mix over the constraint."""
    answer = _call(
        "throughput_accounting_json",
        {
            "company_id": company_id,
            "operating_expenses": operating_expenses,
            "products": products,
            "available_constraint_minutes": available_constraint_minutes,
        },
    )
    if answer is not None:
        return answer

    ranked = []
    for p in products:
        throughput_per_unit = p["selling_price"] - p["material_cost"]
        throughput_per_minute = throughput_per_unit / p["time_on_constraint"] if p["time_on_constraint"] > 0 else 0
        ranked.append(
            {
                "name": p["name"],
                "throughput_per_unit": round(throughput_per_unit, 2),
                "throughput_per_minute": round(throughput_per_minute, 2),
                "demand": p["demand"],
                "time_on_constraint": p["time_on_constraint"],
                "total_throughput_possible": round(throughput_per_unit * p["demand"], 2),
            }
        )

    ranked.sort(key=lambda x: x["throughput_per_minute"], reverse=True)

    remaining_minutes = available_constraint_minutes
    optimal_mix = []
    total_throughput = 0

    for p in ranked:
        minutes_needed = p["demand"] * p["time_on_constraint"]
        if remaining_minutes >= minutes_needed:
            produce = p["demand"]
            remaining_minutes -= minutes_needed
        else:
            produce = int(remaining_minutes / p["time_on_constraint"]) if p["time_on_constraint"] > 0 else 0
            remaining_minutes = 0
        contribution = p["throughput_per_unit"] * produce
        total_throughput += contribution
        optimal_mix.append(
            {
                "product": p["name"],
                "produce": produce,
                "demand": p["demand"],
                "throughput_contribution": round(contribution, 2),
            }
        )

    net_profit = total_throughput - operating_expenses
    roi = (net_profit / operating_expenses * 100) if operating_expenses else 0
    utilization = (
        (available_constraint_minutes - remaining_minutes) / available_constraint_minutes * 100
        if available_constraint_minutes
        else 0
    )

    return {
        "company_id": company_id,
        "total_throughput": round(total_throughput, 2),
        "operating_expenses": round(operating_expenses, 2),
        "net_profit": round(net_profit, 2),
        "roi": round(roi, 2),
        "product_ranking": [{"name": p["name"], "tpm": p["throughput_per_minute"]} for p in ranked],
        "optimal_mix": optimal_mix,
        "constraint_utilization": round(utilization, 1),
    }
