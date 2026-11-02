"""Overhead apportionment.

The figures come from the Rust library when its extension is installed; the
Python maths stays here as the reference implementation and as the fallback for
images built without the wheel. tests/test_cost_accounting_seam.py holds the two
to the same answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

#: The basis fields read off each cost centre; anything else is ignored.
BASIS_FIELDS = ("floor_area", "machine_hours", "personnel", "nbv", "requisitions", "volume")


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


def _centre_numbers(centre):
    """Only the numeric basis fields, so the seam sees the same shape as the Python path."""
    return {field: centre.get(field, 0) for field in BASIS_FIELDS if field in centre}


def apportion_reference(overhead, period, cost_centre_data):
    """Python reference implementation of `/apportion`."""
    basis = overhead["basis"]
    total_basis = sum(c.get(basis, 0) for c in cost_centre_data)
    apportionments = []
    for centre in cost_centre_data:
        basis_value = centre.get(basis, 0)
        if total_basis > 0:
            proportion = basis_value / total_basis
            apportionments.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "basis_value": basis_value,
                    "basis": basis,
                    "proportion": proportion,
                    "apportioned_amount": overhead["total_amount"] * proportion,
                }
            )
    return {
        "overhead_id": overhead["overhead_id"],
        "period": period,
        "total_overhead": overhead["total_amount"],
        "cost_centre_apportionments": apportionments,
    }


def apportion(overhead, period, cost_centre_data):
    """Apportion one overhead across the cost centres by its basis."""
    if engine_in_use() == "rust":
        result = _call(
            "overhead_apportion_json",
            {
                "overhead_id": overhead["overhead_id"],
                "overhead_name": overhead["overhead_name"],
                "total_amount": overhead["total_amount"],
                "basis": overhead["basis"],
                "period": period,
                "cost_centre_data": [
                    {
                        "cost_centre_id": c["cost_centre_id"],
                        "cost_centre_name": c["cost_centre_name"],
                        **_centre_numbers(c),
                    }
                    for c in cost_centre_data
                ],
            },
        )
        if result is not None:
            return result
    return apportion_reference(overhead, period, cost_centre_data)
