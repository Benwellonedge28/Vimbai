"""Order acceptance calculations.

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


def analyze(
    order_id,
    product_id,
    product_name,
    customer_name,
    normal_selling_price,
    offered_price,
    variable_cost_per_unit,
    units_requested,
    full_cost_per_unit=None,
    incremental_fixed_costs=0,
    displaced_sales_units=0,
    displaced_contribution_per_unit=0,
):
    """Accept or reject a special order, with the conditions that decide it."""
    answer = _call(
        "order_acceptance_json",
        {
            "order_id": order_id,
            "product_id": product_id,
            "product_name": product_name,
            "customer_name": customer_name,
            "normal_selling_price": normal_selling_price,
            "offered_price": offered_price,
            "variable_cost_per_unit": variable_cost_per_unit,
            "units_requested": units_requested,
            "full_cost_per_unit": full_cost_per_unit,
            "incremental_fixed_costs": incremental_fixed_costs,
            "displaced_sales_units": displaced_sales_units,
            "displaced_contribution_per_unit": displaced_contribution_per_unit,
        },
    )
    if answer is not None:
        return answer

    price_below_normal = normal_selling_price - offered_price
    price_below_normal_percentage = price_below_normal / normal_selling_price * 100 if normal_selling_price > 0 else 0
    contribution_per_unit_at_normal = normal_selling_price - variable_cost_per_unit
    contribution_per_unit_at_offered = offered_price - variable_cost_per_unit
    contribution_from_order = contribution_per_unit_at_offered * units_requested
    total_relevant_cost = variable_cost_per_unit * units_requested + incremental_fixed_costs
    minimum_acceptable_price = (
        variable_cost_per_unit + (incremental_fixed_costs / units_requested) if units_requested > 0 else 0
    )
    opportunity_cost = displaced_sales_units * displaced_contribution_per_unit
    net_contribution = contribution_from_order - opportunity_cost

    if opportunity_cost == 0:
        if contribution_per_unit_at_offered > 0:
            recommendation = "ACCEPT"
            conditions = ["Positive contribution generated"]
        else:
            recommendation = "REJECT"
            conditions = ["Negative contribution - would lose money"]
    elif net_contribution > 0:
        recommendation = "ACCEPT_WITH_CONDITIONS"
        conditions = [
            f"Net contribution: {net_contribution}",
            "Consider impact on existing customers",
        ]
    elif net_contribution == 0:
        recommendation = "INDIFFERENT"
        conditions = ["Consider strategic value of customer relationship"]
    else:
        recommendation = "REJECT"
        conditions = [
            f"Net contribution negative: {net_contribution}",
            "Would reduce overall profitability",
        ]

    return {
        "order_id": order_id,
        "product_id": product_id,
        "product_name": product_name,
        "customer_name": customer_name,
        "normal_selling_price": normal_selling_price,
        "offered_price": offered_price,
        "variable_cost_per_unit": variable_cost_per_unit,
        "units_requested": units_requested,
        "total_fixed_cost_incremental": incremental_fixed_costs,
        "price_below_normal": price_below_normal,
        "price_below_normal_percentage": price_below_normal_percentage,
        "contribution_per_unit_at_normal": contribution_per_unit_at_normal,
        "contribution_per_unit_at_offered": contribution_per_unit_at_offered,
        "contribution_from_order": contribution_from_order,
        "total_relevant_cost": total_relevant_cost,
        "minimum_acceptable_price": minimum_acceptable_price,
        "opportunity_cost": opportunity_cost,
        "net_contribution": net_contribution,
        "full_cost_per_unit": full_cost_per_unit,
        "recommendation": recommendation,
        "conditions": conditions,
    }


def one_time_order(product_name, offered_price, variable_cost_per_unit, units, has_spare_capacity):
    """Quick one-time-order check against spare capacity."""
    answer = _call(
        "one_time_order_json",
        {
            "product_name": product_name,
            "offered_price": offered_price,
            "variable_cost_per_unit": variable_cost_per_unit,
            "units": units,
            "has_spare_capacity": has_spare_capacity,
        },
    )
    if answer is not None:
        return answer

    contribution_per_unit = offered_price - variable_cost_per_unit
    total_contribution = contribution_per_unit * units
    if has_spare_capacity:
        if contribution_per_unit > 0:
            recommendation = "ACCEPT"
            reason = f"Spare capacity available. Generates {total_contribution} contribution."
        else:
            recommendation = "REJECT"
            reason = "Price below variable cost."
    elif contribution_per_unit > 0:
        recommendation = "CONSIDER"
        reason = "No spare capacity - would need to reduce regular production"
    else:
        recommendation = "REJECT"
        reason = "Price below variable cost"

    return {
        "product_name": product_name,
        "offered_price": offered_price,
        "variable_cost_per_unit": variable_cost_per_unit,
        "units": units,
        "contribution_per_unit": contribution_per_unit,
        "total_contribution": total_contribution,
        "minimum_acceptable_price": variable_cost_per_unit,
        "has_spare_capacity": has_spare_capacity,
        "recommendation": recommendation,
        "reason": reason,
    }
