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
