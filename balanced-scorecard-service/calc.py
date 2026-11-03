"""Balanced scorecard calculations.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

PERSPECTIVES = ("financial", "customer", "internal", "learning_growth")


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


REQUIRED = ("scorecard_json",)


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


def scorecard(company_id, period, kpis):
    """Perspective scores, the overall score and the action items.

    Kept as the service reported it: a perspective score is the weighted
    achievement percentage scaled by 100 again, so scores run to 10,000 and the
    70/90 thresholds almost always read the same way. Correcting the scale would
    move numbers clients have already seen, so it is preserved and pinned by a
    test rather than quietly fixed.
    """
    payload = {"company_id": company_id, "period": period, "kpis": list(kpis)}
    answer = _call("scorecard_json", payload)
    if answer is not None:
        return answer

    perspectives = {key: {"kpis": [], "score": 0.0, "weight_total": 0.0} for key in PERSPECTIVES}
    action_items = []
    for kpi in kpis:
        bucket = perspectives.get(kpi["perspective"], perspectives["financial"])
        achievement = min(kpi["actual"] / kpi["target"], 1.0) * 100 if kpi["target"] > 0 else 100
        bucket["kpis"].append(
            {
                "name": kpi["name"],
                "target": kpi["target"],
                "actual": kpi["actual"],
                "achievement_pct": round(achievement, 1),
                "weight": kpi["weight"],
            }
        )
        bucket["score"] += achievement * kpi["weight"]
        bucket["weight_total"] += kpi["weight"]
        if achievement < 80:
            action_items.append(f"{kpi['name']}: Below target ({achievement:.0f}%) - review strategy")

    for key in perspectives:
        total = perspectives[key]["weight_total"]
        perspectives[key]["score"] = round(perspectives[key]["score"] / total * 100, 1) if total else 0

    scores = [perspectives[key]["score"] for key in PERSPECTIVES]
    overall = round(sum(scores) / 4, 1)
    if overall < 70:
        action_items.insert(0, "Overall performance below 70% - strategic realignment needed")
    elif overall >= 90:
        action_items.insert(0, "Excellent performance across all perspectives")

    return {
        "company_id": company_id,
        "period": period,
        "financial_score": scores[0],
        "customer_score": scores[1],
        "internal_score": scores[2],
        "learning_score": scores[3],
        "overall_score": overall,
        "perspectives": {k: {kk: vv for kk, vv in v.items() if kk != "weight_total"} for k, v in perspectives.items()},
        "action_items": action_items or ["All KPIs on track"],
    }
