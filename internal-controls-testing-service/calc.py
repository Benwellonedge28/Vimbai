"""Internal control testing.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("controls_test_json",)


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    """The extension, but only when it carries the function this service needs.

    An older wheel on the image imports cleanly and then fails at the first call,
    so the probe is the function itself rather than the module.
    """
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


def test_controls(company_id, fiscal_year, controls):
    """Exception rates, deficiency levels and the opinion on the control set.

    Thresholds are the service's own: none at zero exceptions, a deficiency to 5%,
    a significant deficiency to 10%, and a material weakness beyond that.
    """
    payload = {"company_id": company_id, "fiscal_year": fiscal_year, "controls": list(controls)}
    answer = _call("controls_test_json", payload)
    if answer is not None:
        return answer

    effective = deficient = material = significant = 0
    details = []
    remediation = []
    for ctrl in controls:
        rate = ctrl["exceptions_found"] / ctrl["test_sample_size"] if ctrl["test_sample_size"] else 0
        if rate == 0:
            level, effective = "none", effective + 1
        elif rate <= 0.05:
            level, deficient = "deficiency", deficient + 1
        elif rate <= 0.10:
            level, significant, deficient = "significant_deficiency", significant + 1, deficient + 1
        else:
            level, material, deficient = "material_weakness", material + 1, deficient + 1
        details.append(
            {
                "id": ctrl["id"],
                "name": ctrl["name"],
                "type": ctrl["control_type"],
                "process": ctrl["process"],
                "sample_size": ctrl["test_sample_size"],
                "exceptions": ctrl["exceptions_found"],
                "exception_rate": round(rate * 100, 1),
                "deficiency_level": level,
                "effective": level == "none",
                "remediation_needed": level != "none",
                "last_tested": ctrl.get("last_tested", ""),
            }
        )
        if level != "none":
            remediation.append(
                {
                    "control_id": ctrl["id"],
                    "control_name": ctrl["name"],
                    "deficiency_level": level,
                    "exception_rate": round(rate * 100, 1),
                    "recommended_action": f"Redesign {ctrl['name']} - increase sample testing frequency",
                }
            )

    if material > 0:
        opinion = "Adverse opinion - material weaknesses identified"
    elif significant > 0:
        opinion = "Qualified opinion - significant deficiencies identified"
    else:
        opinion = "Unqualified opinion - controls operating effectively"
    return {
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "total_controls": len(controls),
        "controls_tested": len(controls),
        "effective_controls": effective,
        "deficient_controls": deficient,
        "material_weaknesses": material,
        "significant_deficiencies": significant,
        "overall_assessment": opinion,
        "control_details": details,
        "remediation_plan": remediation,
    }
