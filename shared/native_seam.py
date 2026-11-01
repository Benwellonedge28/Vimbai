"""The Python side of the Rust calculation seam.

The services call through this module rather than importing the extension
directly, so there is one place that decides what happens when the wheel is
absent. It is deliberately loud: a missing extension raises instead of falling
back to float maths, because a quiet fallback is how a treasury report ends up
computed two different ways in two environments.

The extension is built from library/native (`maturin build`). Every amount
crosses the boundary as a decimal string.
"""

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
