"""Generated from `contracts/treasury.yaml` by `scripts/generate_sdk.py`. Do not edit.

Where the cash is sitting, and what moving it would earn.

Amounts travel as decimal strings in both directions, as the contract says. The client refuses a
float: a float has already lost digits by the time it is passed, and an amount that quietly changed
is worse than an error.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from decimal import Decimal
from typing import Any, Mapping, Optional


class VimbaiCashOptimisationClientError(Exception):
    """A refusal from the service, carrying the sentence it was refused with."""

    def __init__(self, status: int, message: str) -> None:
        super().__init__("{}: {}".format(status, message))
        self.status = status
        self.message = message


def _money(name: str, value: Any) -> str:
    """An amount as the exact string the service expects, or a refusal."""
    if isinstance(value, float):
        raise TypeError(
            "{} must be a decimal string, an int or a Decimal, not a float: a float has already "
            "lost digits. Pass it as a string.".format(name)
        )
    if isinstance(value, bool) or not isinstance(value, (str, int, Decimal)):
        raise TypeError("{} must be a decimal string, an int or a Decimal, not {}".format(name, type(value).__name__))
    return str(value)


class VimbaiCashOptimisationClient:
    """Where the cash is sitting, and what moving it would earn.

    Both headers are required, and there is no default for either: a rate that cannot say whose Book
    it belongs to has no meaning, so the client refuses to be built without them.
    """

    def __init__(
        self,
        base_url: str,
        *,
        user_id: str,
        book_id: str,
        timeout: Optional[float] = 30.0,
        transport: Optional[Any] = None,
    ) -> None:
        if not user_id or not book_id:
            raise ValueError("user_id and book_id are both required")
        self._base_url = base_url.rstrip("/")
        self._headers = {"X-User-Id": user_id, "X-Book-Id": book_id}
        self._timeout = timeout
        self._transport = transport or urllib.request.urlopen

    def _call(
        self,
        method: str,
        path: str,
        query: Optional[Mapping[str, Any]] = None,
        body: Optional[Mapping[str, Any]] = None,
    ) -> Any:
        url = self._base_url + path
        if query:
            url = url + "?" + urllib.parse.urlencode(query)
        data = None if body is None else json.dumps(body).encode("utf-8")
        headers = dict(self._headers)
        if data is not None:
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(url, data=data, method=method, headers=headers)
        try:
            with self._transport(request, timeout=self._timeout) as response:
                payload = response.read()
        except urllib.error.HTTPError as error:
            raw = error.read()
            try:
                message = json.loads(raw).get("error", raw.decode("utf-8", "replace"))
            except (ValueError, AttributeError):
                message = raw.decode("utf-8", "replace")
            raise VimbaiCashOptimisationClientError(error.code, message) from None
        return json.loads(payload) if payload else None

    def add_cash_account(
        self,
        company_id: Any,
        account_name: Any,
        account_type: Any,
        balance: Any,
        min_required: Optional[Any] = None,
        interest_rate: Optional[Any] = None,
        currency: Optional[Any] = None,
    ) -> Any:
        """Add a cash account with its balance and the minimum it must keep."""
        body: dict[str, Any] = {}
        if company_id is not None:
            body["company_id"] = company_id
        if account_name is not None:
            body["account_name"] = account_name
        if account_type is not None:
            body["account_type"] = account_type
        if balance is not None:
            body["balance"] = _money("balance", balance)
        if min_required is not None:
            body["min_required"] = _money("min_required", min_required)
        if interest_rate is not None:
            body["interest_rate"] = _money("interest_rate", interest_rate)
        if currency is not None:
            body["currency"] = currency
        query = None
        path = "/cash-optimization/accounts"
        return self._call("POST", path, query=query, body=body)

    def list_cash_accounts(
        self,
        company_id: str,
    ) -> Any:
        """The caller's accounts for one company, oldest first."""
        query = None
        path = f"/cash-optimization/accounts/{company_id}"
        return self._call("GET", path, query=query, body=None)

    def optimize_cash(
        self,
        company_id: str,
    ) -> Any:
        """Recompute where the cash should move and replace the stored suggestions."""
        query = None
        path = f"/cash-optimization/optimize/{company_id}"
        return self._call("POST", path, query=query, body=None)

    def list_cash_suggestions(
        self,
        company_id: str,
    ) -> Any:
        """The suggestions as last computed, oldest first."""
        query = None
        path = f"/cash-optimization/suggestions/{company_id}"
        return self._call("GET", path, query=query, body=None)


__all__ = ["VimbaiCashOptimisationClient", "VimbaiCashOptimisationClientError"]
