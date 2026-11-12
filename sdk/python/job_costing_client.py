"""Generated from `contracts/job-costing.yaml` by `scripts/generate_sdk.py`. Do not edit.

A job, the costs charged to it, and what is left of the contract value.

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


class VimbaiJobCostingClientError(Exception):
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


class VimbaiJobCostingClient:
    """A job, the costs charged to it, and what is left of the contract value.

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
            raise VimbaiJobCostingClientError(error.code, message) from None
        return json.loads(payload) if payload else None

    def open_job(
        self,
        company_id: Any,
        contract_value: Any,
        status: Optional[Any] = None,
        job_name: Optional[Any] = None,
        customer_name: Optional[Any] = None,
        description: Optional[Any] = None,
    ) -> Any:
        """Open a job with its contract value."""
        body: dict[str, Any] = {}
        if company_id is not None:
            body["company_id"] = company_id
        if contract_value is not None:
            body["contract_value"] = _money("contract_value", contract_value)
        if status is not None:
            body["status"] = status
        if job_name is not None:
            body["job_name"] = job_name
        if customer_name is not None:
            body["customer_name"] = customer_name
        if description is not None:
            body["description"] = description
        query = None
        path = "/job-costing/jobs"
        return self._call("POST", path, query=query, body=body)

    def list_jobs(
        self,
        company_id: str,
        status: Optional[str] = None,
    ) -> Any:
        """The caller's jobs for one company, optionally by status."""
        query = {
            "status": status,
        }
        query = {key: value for key, value in query.items() if value is not None}
        path = f"/job-costing/jobs/{company_id}"
        return self._call("GET", path, query=query, body=None)

    def charge_cost(
        self,
        job_id: str,
        amount: Any,
        cost_type: Optional[Any] = None,
        description: Optional[Any] = None,
        reference: Optional[Any] = None,
    ) -> Any:
        """Charge a cost to a job and move its running total."""
        body: dict[str, Any] = {}
        if amount is not None:
            body["amount"] = _money("amount", amount)
        if cost_type is not None:
            body["cost_type"] = cost_type
        if description is not None:
            body["description"] = description
        if reference is not None:
            body["reference"] = reference
        query = None
        path = f"/job-costing/jobs/{job_id}/costs"
        return self._call("POST", path, query=query, body=body)

    def job_profitability(
        self,
        company_id: str,
    ) -> Any:
        """The company's jobs, summed exactly."""
        query = None
        path = f"/job-costing/jobs/{company_id}/profitability"
        return self._call("GET", path, query=query, body=None)


__all__ = ["VimbaiJobCostingClient", "VimbaiJobCostingClientError"]
