"""Generated from `contracts/equity.yaml` by `scripts/generate_sdk.py`. Do not edit.

What moved shareholders' equity, and the eight figures that reconcile it.

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


class VimbaiEquityChangesClientError(Exception):
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


class VimbaiEquityChangesClient:
    """What moved shareholders' equity, and the eight figures that reconcile it.

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
            raise VimbaiEquityChangesClientError(error.code, message) from None
        return json.loads(payload) if payload else None

    def record_equity_transaction(
        self,
        company_id: Any,
        transaction_type: Any,
        shareholder: Optional[Any] = None,
        shares: Optional[Any] = None,
        price_per_share: Optional[Any] = None,
        amount: Optional[Any] = None,
        description: Optional[Any] = None,
        date: Optional[Any] = None,
        created_at: Optional[Any] = None,
    ) -> Any:
        """Record one equity transaction."""
        body: dict[str, Any] = {}
        if company_id is not None:
            body["company_id"] = company_id
        if transaction_type is not None:
            body["transaction_type"] = transaction_type
        if shareholder is not None:
            body["shareholder"] = shareholder
        if shares is not None:
            body["shares"] = shares
        if price_per_share is not None:
            body["price_per_share"] = _money("price_per_share", price_per_share)
        if amount is not None:
            body["amount"] = _money("amount", amount)
        if description is not None:
            body["description"] = description
        if date is not None:
            body["date"] = date
        if created_at is not None:
            body["created_at"] = created_at
        query = None
        path = "/equity-changes/transactions"
        return self._call("POST", path, query=query, body=body)

    def list_equity_transactions(
        self,
        company_id: str,
        tx_type: Optional[str] = None,
    ) -> Any:
        """The caller's transactions for one company, optionally filtered by type."""
        query = {
            "tx_type": tx_type,
        }
        query = {key: value for key, value in query.items() if value is not None}
        path = f"/equity-changes/transactions/{company_id}"
        return self._call("GET", path, query=query, body=None)

    def generate_equity_statement(
        self,
        company_id: Any,
        period: Any,
        beginning_equity: Optional[Any] = None,
        transactions: Optional[Any] = None,
        created_at: Optional[Any] = None,
    ) -> Any:
        """Roll transactions up by type, derive the ending equity, and store the statement."""
        body: dict[str, Any] = {}
        if company_id is not None:
            body["company_id"] = company_id
        if period is not None:
            body["period"] = period
        if beginning_equity is not None:
            body["beginning_equity"] = _money("beginning_equity", beginning_equity)
        if transactions is not None:
            body["transactions"] = transactions
        if created_at is not None:
            body["created_at"] = created_at
        query = None
        path = "/equity-changes/statement"
        return self._call("POST", path, query=query, body=body)

    def list_equity_statements(
        self,
        company_id: str,
    ) -> Any:
        """Every statement for the company, with the count."""
        query = None
        path = f"/equity-changes/statements/{company_id}"
        return self._call("GET", path, query=query, body=None)


__all__ = ["VimbaiEquityChangesClient", "VimbaiEquityChangesClientError"]
