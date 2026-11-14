"""Every address a retired service published must still be answered by the engine that answers for it.

The gateway entry for a retired service survives, so its URL keeps working only if the engine registers the address the
gateway forwards - the service path with or without the prefix, depending on strip_prefix. Nothing checked that until
this test; nine services had drifted, most of them on parameterised paths a fixed-path survey never looked at.
"""

from scripts.gateway_addresses import audit, stale_gaps


def test_the_engine_answers_every_address_a_retired_service_published():
    findings = audit()
    assert not findings, "\n".join(findings)


def test_no_known_gap_has_quietly_been_fixed():
    assert not stale_gaps(), "\n".join(stale_gaps())


def test_no_new_stripped_path_collisions():
    """Two engine-served services must never claim the same stripped path.

    One engine means one handler per address. When two retired services both publish `/summary/{company}` and both have
    their prefix stripped, the second service's callers are answered by the first service's handler - a 200 with the
    wrong numbers. That is worse than a 404, so it is pinned: today's collisions are listed, and closing one means
    removing it from the list.
    """
    import json
    import os

    from scripts.gateway_addresses import ROOT, stripped_collisions

    with open(os.path.join(ROOT, "infrastructure", "kubernetes", "stripped-path-collisions.json")) as handle:
        pinned = json.load(handle)["collisions"]
    found = stripped_collisions()
    new = {path: names for path, names in found.items() if path not in pinned}
    assert not new, f"two engine-served services claim one stripped path: {new}"
    fixed = sorted(set(pinned) - set(found))
    assert not fixed, f"collision resolved, remove it from the pinned list: {fixed}"


def test_every_published_method_is_answered_not_just_the_address():
    """A path answered by the right address and the wrong verb still fails the caller.

    The shape check reads addresses only, so a listing published as GET while the engine binds POST looked answered for
    as long as nobody called it. This reads the methods back out of the service's last main.py and out of the engine,
    and the three remaining gaps are listed with what closing each one needs.
    """
    from scripts.gateway_addresses import method_gaps

    findings = method_gaps()
    assert not findings, "\n".join(findings)
