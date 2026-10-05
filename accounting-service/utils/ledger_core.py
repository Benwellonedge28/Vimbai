"""Ledger kernel adapter for the accounting service.

First choice: the Rust kernel `vimbai_ledger_core` (PyO3 wheel, 100% safe
Rust). Fallback: a pure-Python implementation of the same rules so tests
and environments without the wheel stay correct. Both produce IDENTICAL
stamps (canonical JSON: sorted keys, compact separators, SHA-256 over
`prev_hash || payload`), so a chain started under one backend verifies
under the other.

Shared contract (all raise ValueError with a JSON error list on
violation, mirroring the Rust kernel):
- validate_double_entry([(debit, credit), ...]) -> True
- genesis_hash() -> str
- hash_entry(payload: dict, prev_hash: str | None) -> str
- verify_chain([{entry_id, payload, prev_hash?, stored_hash?}, ...]) -> str (JSON report)
- check_reversal_mirror([account/debit/credit dicts], [...]) -> True
- flatten_lines_by_account([(debit, credit), ...], [accounts]) -> str (JSON)
"""

import hashlib
import json

GENESIS = "0" * 64
_EPS = 1e-9

try:  # pragma: no cover - depends on environment
    from vimbai_ledger_core import (  # type: ignore
        check_reversal_mirror,
        flatten_lines_by_account,
        genesis_hash,
        hash_entry,
        validate_double_entry,
        verify_chain,
    )

    BACKEND = "rust"
except ImportError:
    BACKEND = "python-fallback"

    def genesis_hash() -> str:
        return GENESIS

    def _canonical(payload) -> str:
        return json.dumps(payload, sort_keys=True, separators=(",", ":"))

    def _err(errors) -> None:
        raise ValueError(json.dumps(errors))

    def validate_double_entry(lines) -> bool:
        errors = []
        if len(lines) < 2:
            errors.append({"code": "too_few_lines", "detail": {"lines": len(lines)}})
        total_debits = 0.0
        total_credits = 0.0
        structural = False
        for i, (debit, credit) in enumerate(lines):
            d, c = float(debit), float(credit)
            if d != d or c != c or d in (float("inf"),) or c in (float("inf"),) or d < 0 or c < 0:
                errors.append({"code": "invalid_amount", "detail": {"line": i}})
                structural = True
                continue
            moves_debit, moves_credit = d > 0, c > 0
            if moves_debit and moves_credit:
                errors.append({"code": "line_moves_both_sides", "detail": {"line": i}})
                structural = True
                continue
            if not moves_debit and not moves_credit:
                errors.append({"code": "line_moves_nothing", "detail": {"line": i}})
                structural = True
                continue
            total_debits += d
            total_credits += c
        if not structural and lines:
            diff = abs(total_debits - total_credits)
            if diff > _EPS:
                if diff > 0.01:
                    errors.append(
                        {
                            "code": "unbalanced",
                            "detail": {"debits": total_debits, "credits": total_credits},
                        }
                    )
                else:
                    errors.append({"code": "tolerance_exceeded", "detail": {"diff": diff}})
        if errors:
            _err(errors)
        return True

    def hash_entry(payload: dict, prev_hash: str = None) -> str:
        prev = prev_hash or GENESIS
        digest = hashlib.sha256((prev + _canonical(payload)).encode()).hexdigest()
        return digest

    def verify_chain(entries) -> str:
        errors = []
        prev = GENESIS
        head = GENESIS
        for e in entries:
            payload = e["payload"]
            stored_prev = e.get("prev_hash") or GENESIS
            if stored_prev != prev:
                errors.append(
                    {
                        "code": "broken_link",
                        "detail": {
                            "entry_id": e["entry_id"],
                            "prev_in_chain": prev,
                            "stored_prev": stored_prev,
                        },
                    }
                )
            computed = hash_entry(payload, stored_prev)
            stored = e.get("stored_hash")
            if stored is not None and stored != computed:
                errors.append(
                    {
                        "code": "hash_mismatch",
                        "detail": {
                            "entry_id": e["entry_id"],
                            "expected": computed,
                            "actual": stored,
                        },
                    }
                )
            prev = computed
            head = computed
        return json.dumps(
            {
                "valid": not errors,
                "entries_checked": len(entries),
                "head_hash": head,
                "errors": errors,
            }
        )

    def check_reversal_mirror(original, reversal) -> bool:
        errors = []
        orig = {m["account"]: m for m in original}
        rev = {m["account"]: m for m in reversal}
        if not orig:
            _err([{"code": "empty_original", "detail": {}}])
        if set(orig) != set(rev):
            errors.append(
                {
                    "code": "account_set_differs",
                    "detail": {
                        "original": sorted(orig),
                        "reversal": sorted(rev),
                    },
                }
            )
            _err(errors)
        for account, o in orig.items():
            r = rev.get(account)
            if r is None:
                continue
            if abs(o["credit"] - r["debit"]) >= _EPS or abs(o["debit"] - r["credit"]) >= _EPS:
                errors.append(
                    {
                        "code": "not_mirrored",
                        "detail": {
                            "account": account,
                            "original": o["debit"] - o["credit"],
                            "reversal": r["debit"] - r["credit"],
                        },
                    }
                )
        if errors:
            _err(errors)
        return True

    def flatten_lines_by_account(lines, accounts) -> str:
        per_account: dict = {}
        for i, (debit, credit) in enumerate(lines):
            account = accounts[i] if i < len(accounts) else ""
            d, c = per_account.get(account, (0.0, 0.0))
            per_account[account] = (d + float(debit), c + float(credit))
        flat = [{"account": a, "debit": v[0], "credit": v[1]} for a, v in sorted(per_account.items())]
        return json.dumps(flat)


def build_entry_payload(
    entry_id: str,
    entry_date: str,
    description: str,
    reference_number,
    source_module: str,
    status: str,
    lines,
) -> dict:
    """Canonical business payload for a journal entry stamp.

    `lines` is a list of dicts: {account, debit, credit, description}.
    Must stay byte-identical between stamp-time and verify-time.
    """
    return {
        "id": entry_id,
        "entry_date": entry_date,
        "description": description,
        "reference_number": reference_number,
        "source_module": source_module,
        "status": status,
        "lines": [
            {
                "account": l["account"],
                "debit": float(l["debit"]),
                "credit": float(l["credit"]),
                "description": l["description"],
            }
            for l in lines
        ],
    }
