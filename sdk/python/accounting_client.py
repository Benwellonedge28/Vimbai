"""Generated from `contracts/accounting.yaml` by `scripts/generate_sdk.py`. Do not edit.

A statement of the cash that moved, and the five figures that have to tie out.

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


class VimbaiCashFlowStatementClientError(Exception):
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


class VimbaiCashFlowStatementClient:
    """A statement of the cash that moved, and the five figures that have to tie out.

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
            raise VimbaiCashFlowStatementClientError(error.code, message) from None
        return json.loads(payload) if payload else None

    def generate_cash_flow_statement(
        self,
        company_id: Any,
        method: Optional[Any] = None,
        period_start: Optional[Any] = None,
        period_end: Optional[Any] = None,
        beginning_cash: Optional[Any] = None,
        operating_activities: Optional[Any] = None,
        investing_activities: Optional[Any] = None,
        financing_activities: Optional[Any] = None,
        created_at: Optional[Any] = None,
    ) -> Any:
        """Compute the totals from the lines and store the statement."""
        body: dict[str, Any] = {}
        if company_id is not None:
            body["company_id"] = company_id
        if method is not None:
            body["method"] = method
        if period_start is not None:
            body["period_start"] = period_start
        if period_end is not None:
            body["period_end"] = period_end
        if beginning_cash is not None:
            body["beginning_cash"] = _money("beginning_cash", beginning_cash)
        if operating_activities is not None:
            body["operating_activities"] = operating_activities
        if investing_activities is not None:
            body["investing_activities"] = investing_activities
        if financing_activities is not None:
            body["financing_activities"] = financing_activities
        if created_at is not None:
            body["created_at"] = created_at
        query = None
        path = "/cash-flow-statement/generate"
        return self._call("POST", path, query=query, body=body)

    def latest_cash_flow_statement(
        self,
        company_id: str,
    ) -> Any:
        """The most recent statement, or a miss when there is none."""
        query = None
        path = f"/cash-flow-statement/latest/{company_id}"
        return self._call("GET", path, query=query, body=None)

    def cash_flow_history(
        self,
        company_id: str,
    ) -> Any:
        """Every statement for the company, oldest first."""
        query = None
        path = f"/cash-flow-statement/history/{company_id}"
        return self._call("GET", path, query=query, body=None)


__all__ = ["VimbaiCashFlowStatementClient", "VimbaiCashFlowStatementClientError"]
