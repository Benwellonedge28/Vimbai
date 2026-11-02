"""Margin of safety arithmetic.

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


def _risk_level(percentage):
    """Bands are strictly >= 20 / >= 10: exactly 20% is low, exactly 10% is medium."""
    if percentage >= 20:
        return "low"
    if percentage >= 10:
        return "medium"
    return "high"


def margin_of_safety_reference(current_output, current_revenue, break_even_output, break_even_revenue):
    """Python reference implementation of `/calculate`."""
    units = current_output - break_even_output
    revenue = current_revenue - break_even_revenue
    percentage = (revenue / current_revenue * 100) if current_revenue > 0 else 0
    ratio = (units / current_output) if current_output > 0 else 0
    return {
        "current_output": current_output,
        "current_revenue": current_revenue,
        "break_even_output": break_even_output,
        "break_even_revenue": break_even_revenue,
        "margin_of_safety_units": units,
        "margin_of_safety_revenue": revenue,
        "margin_of_safety_percentage": percentage,
        "margin_of_safety_ratio": ratio,
        "risk_level": _risk_level(percentage),
    }


def margin_of_safety(current_output, current_revenue, break_even_output, break_even_revenue):
    """Margin of safety from stated break-even figures."""
    if engine_in_use() == "rust":
        result = _call(
            "margin_of_safety_json",
            {
                "current_output": current_output,
                "current_revenue": current_revenue,
                "break_even_output": break_even_output,
                "break_even_revenue": break_even_revenue,
            },
        )
        if result is not None:
            return result
    return margin_of_safety_reference(current_output, current_revenue, break_even_output, break_even_revenue)


def margin_from_data_reference(selling_price, fixed_costs, variable_cost_per_unit, expected_sales_units):
    """Python reference implementation of `/calculate-from-data`."""
    contribution = selling_price - variable_cost_per_unit
    break_even_units = (fixed_costs / contribution) if contribution > 0 else 0
    break_even_revenue = break_even_units * selling_price
    current_revenue = expected_sales_units * selling_price
    mos_units = expected_sales_units - break_even_units
    mos_revenue = current_revenue - break_even_revenue
    percentage = (mos_revenue / current_revenue * 100) if current_revenue > 0 else 0
    return {
        "current_sales_units": expected_sales_units,
        "current_revenue": current_revenue,
        "break_even_units": break_even_units,
        "break_even_revenue": break_even_revenue,
        "margin_of_safety_units": mos_units,
        "margin_of_safety_revenue": mos_revenue,
        "margin_of_safety_percentage": percentage,
        "risk_level": _risk_level(percentage),
    }


def margin_from_data(selling_price, fixed_costs, variable_cost_per_unit, expected_sales_units):
    """Margin of safety derived from price, cost and volume."""
    if engine_in_use() == "rust":
        result = _call(
            "margin_from_data_json",
            {
                "selling_price": selling_price,
                "fixed_costs": fixed_costs,
                "variable_cost_per_unit": variable_cost_per_unit,
                "expected_sales_units": expected_sales_units,
            },
        )
        if result is not None:
            return result
    return margin_from_data_reference(selling_price, fixed_costs, variable_cost_per_unit, expected_sales_units)


def target_mos_reference(break_even_revenue, target_mos_percentage):
    """Python reference implementation of `/target-mos`.

    A target of 100% or more has no finite answer, which JSON cannot carry, so it
    is reported as None. The original returned a float infinity that failed to
    serialise and answered 500; that crash is pinned in the service tests.
    """
    if target_mos_percentage < 100:
        required = break_even_revenue / (1 - target_mos_percentage / 100)
    else:
        required = None
    return {
        "break_even_revenue": break_even_revenue,
        "target_mos_percentage": target_mos_percentage,
        "required_sales_revenue": required,
    }


def target_mos(break_even_revenue, target_mos_percentage):
    """Sales revenue needed to reach a target margin of safety."""
    if engine_in_use() == "rust":
        result = _call(
            "target_mos_json",
            {
                "break_even_revenue": break_even_revenue,
                "target_mos_percentage": target_mos_percentage,
            },
        )
        if result is not None:
            return result
    return target_mos_reference(break_even_revenue, target_mos_percentage)
