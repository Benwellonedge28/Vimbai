"""The Python side of the Rust calculation seam.

The services call through this module rather than importing the extension
directly, so there is one place that decides what happens when the wheel is
absent. It is deliberately loud: a missing extension raises instead of falling
back to float maths, because a quiet fallback is how a treasury report ends up
computed two different ways in two environments.

The extension is built from library/native (`maturin build`). Every amount
crosses the boundary as a decimal string.
"""

import json
from decimal import Decimal
from typing import Any, Dict, List, Sequence


class NativeUnavailable(RuntimeError):
    """Raised when the compiled extension is not installed."""


def _extension():
    try:
        import vimbai_native
    except ImportError as exc:  # pragma: no cover - exercised by the guard test
        raise NativeUnavailable(
            "the vimbai_native extension is not installed; build it with "
            "`maturin build --release` in library/native and install the wheel"
        ) from exc
    return vimbai_native


def available() -> bool:
    try:
        _extension()
    except NativeUnavailable:
        return False
    return True


def version() -> str:
    return _extension().__version__


def _amount(value: Any) -> str:
    """Render any accepted amount as the exact decimal string the seam expects."""
    if isinstance(value, Decimal):
        return format(value, "f")
    if isinstance(value, float):
        # floats are refused upstream of this module, but a caller passing one
        # should see its shortest exact form rather than 0.30000000000000004
        return format(Decimal(repr(value)), "f")
    return str(value)


def sum_amounts(values: Sequence[Any]) -> Decimal:
    """Sum amounts exactly, in the order given."""
    return Decimal(_extension().sum_amounts([_amount(v) for v in values]))


def allocate(total: Any, weights: Sequence[Any], scale: int = 2) -> List[Decimal]:
    """Split `total` across `weights` without losing a cent."""
    parts = _extension().allocate(_amount(total), [_amount(w) for w in weights], scale)
    return [Decimal(p) for p in parts]


def treasury_analytics(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Run the treasury KPI set. Returns the same shape the service exposes."""
    import json

    exact = {}
    for key, value in payload.items():
        exact[key] = value if isinstance(value, str) else _amount(value)
    return json.loads(_extension().analyze_treasury_json(json.dumps(exact)))


# --- cost accounting -------------------------------------------------------
#
# Absorption, marginal and target costing. The services keep their Python maths
# as the reference implementation; these wrappers reach the Rust library when
# the extension is installed.


def _call(name: str, payload: Dict[str, Any]) -> Dict[str, Any]:
    """Call one seam function with a JSON payload and return its JSON answer."""
    raw = getattr(_extension(), name)(json.dumps(payload))
    return json.loads(raw)


def absorption_product_cost(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Product cost card: prime cost, total production cost, cost per unit."""
    return _call("absorption_product_cost_json", payload)


def overhead_absorption(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Overhead absorption rate and the overhead it absorbs."""
    return _call("absorption_overhead_json", payload)


def cost_plus_pricing(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Cost-plus pricing: markup amount and selling price."""
    return _call("absorption_cost_plus_json", payload)


def target_cost(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Target costing, including the per-component reduction analysis.

    `component_costs` must be `[[name, cost], ...]` pairs so the analysis keeps
    the caller's insertion order.
    """
    return _call("target_cost_json", payload)


def marginal_cost_item(payload: Dict[str, Any]) -> Dict[str, Any]:
    """A marginal cost item with its total and unit variable cost."""
    return _call("marginal_cost_item_json", payload)


def marginal_income_statement(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Marginal income statement: total variable costs, contribution, profit."""
    return _call("marginal_income_statement_json", payload)


def marginal_contribution(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Product contribution analysis."""
    return _call("marginal_contribution_json", payload)


def marginal_forecast(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Profit forecast under marginal costing."""
    return _call("marginal_forecast_json", payload)


def prime_cost(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Prime cost from explicit components, with the cost breakdown."""
    return _call("prime_cost_json", payload)


def prime_cost_from_items(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Prime cost aggregated from direct cost items for one product."""
    return _call("prime_cost_from_items_json", payload)


def throughput_accounting(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Throughput analysis: ranking, optimal mix and constraint utilization."""
    return _call("throughput_accounting_json", payload)


def limiting_factor(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Optimal production plan under a single scarce resource."""
    return _call("limiting_factor_analyze_json", payload)


def limiting_factor_compare(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Products ranked by contribution per unit of the scarce resource."""
    return _call("limiting_factor_compare_json", payload)


def order_acceptance(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Accept or reject a special order, with the conditions that decide it."""
    return _call("order_acceptance_json", payload)


def one_time_order(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Quick one-time-order check against spare capacity."""
    return _call("one_time_order_json", payload)
