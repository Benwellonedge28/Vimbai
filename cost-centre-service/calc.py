"""Cost centre period summary.

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


def summary_reference(costs, cost_centre_id, period, allocated_service_costs=0.0):
    """Python reference implementation of `/summary/generate`.

    Anything that is not "direct" or "indirect" lands in the overhead bucket,
    which is the original service's behaviour.
    """
    totals = {"direct": 0.0, "indirect": 0.0, "overhead": 0.0}
    for cost in costs:
        if cost["cost_centre_id"] != cost_centre_id or cost["period"] != period:
            continue
        bucket = cost["cost_type"] if cost["cost_type"] in ("direct", "indirect") else "overhead"
        totals[bucket] += cost["amount"]

    total_cost = totals["direct"] + totals["indirect"] + totals["overhead"] + allocated_service_costs
    return {
        "cost_centre_id": cost_centre_id,
        "period": period,
        "total_direct_costs": totals["direct"],
        "total_indirect_costs": totals["indirect"],
        "total_overhead": totals["overhead"],
        "allocated_service_costs": allocated_service_costs,
        "total_cost": total_cost,
    }


def summary(costs, cost_centre_id, period, allocated_service_costs=0.0):
    """Summarise one cost centre for one period."""
    if engine_in_use() == "rust":
        result = _call(
            "cost_centre_summary_json",
            {
                "cost_centre_id": cost_centre_id,
                "period": period,
                "allocated_service_costs": allocated_service_costs,
                "costs": [
                    {
                        "cost_centre_id": c["cost_centre_id"],
                        "cost_code": c["cost_code"],
                        "cost_description": c["cost_description"],
                        "amount": c["amount"],
                        "cost_type": c["cost_type"],
                        "period": c["period"],
                    }
                    for c in costs
                ],
            },
        )
        if result is not None:
            return result
    return summary_reference(costs, cost_centre_id, period, allocated_service_costs)
