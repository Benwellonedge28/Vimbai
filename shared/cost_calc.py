"""Cost accounting calculations shared by the cost services.

The figures come from the Rust library when its extension is installed; the
Python maths stays here as the reference implementation and as the fallback for
images built without the wheel. tests/test_cost_accounting_seam.py holds the two
to the same answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import os


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _seam():
    try:
        from shared import native_seam
    except ImportError:  # pragma: no cover - the services always ship ../shared
        return None
    return native_seam if native_seam.available() else None


def engine_in_use() -> str:
    """Announce the calculator so /health can be read in production."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    if requested in ("python", "reference"):
        return "python"
    if _seam() is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is not installed")
        return "python"
    return "rust"


def marginal_cost_item(name, code, variable_cost, fixed_cost, units, driver):
    """A marginal cost item: total cost and unit variable cost."""
    seam = _seam()
    if seam is not None:
        answer = seam.marginal_cost_item(
            {
                "cost_name": name,
                "cost_code": code,
                "variable_cost": variable_cost,
                "fixed_cost": fixed_cost,
                "units": units,
                "cost_driver": driver,
            }
        )
        return {
            "total_cost": answer["total_cost"],
            "unit_variable_cost": answer["unit_variable_cost"],
        }

    return {
        "total_cost": variable_cost + fixed_cost,
        "unit_variable_cost": variable_cost / units if units > 0 else 0,
    }


def marginal_income_statement(company_id, period, sales_revenue, variable_costs, fixed_costs):
    """Total variable costs, contribution and profit for a period."""
    seam = _seam()
    if seam is not None:
        answer = seam.marginal_income_statement(
            {
                "company_id": company_id,
                "period": period,
                "sales_revenue": sales_revenue,
                "variable_costs": dict(variable_costs),
                "fixed_costs": fixed_costs,
            }
        )
        return {
            "total_variable_costs": answer["total_variable_costs"],
            "contribution": answer["contribution"],
            "profit": answer["profit"],
        }

    total_variable_costs = sum(variable_costs.values())
    contribution = sales_revenue - total_variable_costs
    return {
        "total_variable_costs": total_variable_costs,
        "contribution": contribution,
        "profit": contribution - fixed_costs,
    }


def marginal_contribution(product_id, selling_price, variable_cost_per_unit, units_sold, fixed_costs):
    """Per-unit contribution, margin ratio, total contribution and profit."""
    seam = _seam()
    if seam is not None:
        answer = seam.marginal_contribution(
            {
                "product_id": product_id,
                "selling_price": selling_price,
                "variable_cost_per_unit": variable_cost_per_unit,
                "units_sold": units_sold,
                "fixed_costs": fixed_costs,
            }
        )
        return {
            "contribution_per_unit": answer["contribution_per_unit"],
            "contribution_margin_ratio": answer["contribution_margin_ratio"],
            "total_contribution": answer["total_contribution"],
            "profit_from_product": answer["profit_from_product"],
        }

    contribution_per_unit = selling_price - variable_cost_per_unit
    return {
        "contribution_per_unit": contribution_per_unit,
        "contribution_margin_ratio": ((contribution_per_unit / selling_price) * 100 if selling_price > 0 else 0),
        "total_contribution": contribution_per_unit * units_sold,
        "profit_from_product": contribution_per_unit * units_sold - fixed_costs,
    }


def marginal_forecast(selling_price, variable_cost_per_unit, fixed_costs, expected_units):
    """Forecast profit under marginal costing."""
    seam = _seam()
    if seam is not None:
        answer = seam.marginal_forecast(
            {
                "selling_price": selling_price,
                "variable_cost_per_unit": variable_cost_per_unit,
                "fixed_costs": fixed_costs,
                "expected_units": expected_units,
            }
        )
        return {
            "contribution_per_unit": answer["contribution_per_unit"],
            "contribution_margin_ratio": answer["contribution_margin_ratio"],
            "total_contribution": answer["total_contribution"],
            "forecast_profit": answer["forecast_profit"],
        }

    contribution_per_unit = selling_price - variable_cost_per_unit
    total_contribution = contribution_per_unit * expected_units
    return {
        "contribution_per_unit": contribution_per_unit,
        "contribution_margin_ratio": (contribution_per_unit / selling_price * 100 if selling_price > 0 else 0),
        "total_contribution": total_contribution,
        "forecast_profit": total_contribution - fixed_costs,
    }
