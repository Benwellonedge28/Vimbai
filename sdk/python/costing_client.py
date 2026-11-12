"""Generated from `contracts/costing.yaml` by `scripts/generate_sdk.py`. Do not edit.

Absorption rates for a cost centre, and the overhead a rate recovers.

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


class VimbaiOverheadAbsorptionClientError(Exception):
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


class VimbaiOverheadAbsorptionClient:
    """Absorption rates for a cost centre, and the overhead a rate recovers.

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
            raise VimbaiOverheadAbsorptionClientError(error.code, message) from None
        return json.loads(payload) if payload else None

    def calculate_absorption_rate(
        self,
        cost_centre_name: Any,
        cost_centre_id: Any,
        budgeted_overhead: Any,
        budgeted_base_units: Any,
        overhead_type: Optional[Any] = None,
        absorption_base: Optional[Any] = None,
    ) -> Any:
        """Work out the absorption rate for one cost centre and record it."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if cost_centre_id is not None:
            body["cost_centre_id"] = cost_centre_id
        if budgeted_overhead is not None:
            body["budgeted_overhead"] = _money("budgeted_overhead", budgeted_overhead)
        if budgeted_base_units is not None:
            body["budgeted_base_units"] = _money("budgeted_base_units", budgeted_base_units)
        if overhead_type is not None:
            body["overhead_type"] = overhead_type
        if absorption_base is not None:
            body["absorption_base"] = absorption_base
        query = None
        path = "/overhead-absorption/calculate"
        return self._call("POST", path, query=query, body=body)

    def machine_hours_rate(
        self,
        cost_centre_name: Any,
        budgeted_overhead: Any,
        budgeted_machine_hours: Optional[Any] = None,
        budgeted_direct_labour_hours: Optional[Any] = None,
        budgeted_direct_labour_cost: Optional[Any] = None,
        budgeted_material_cost: Optional[Any] = None,
        budgeted_prime_cost: Optional[Any] = None,
        budgeted_units: Optional[Any] = None,
    ) -> Any:
        """Absorption rate per machine hour."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if budgeted_overhead is not None:
            body["budgeted_overhead"] = _money("budgeted_overhead", budgeted_overhead)
        if budgeted_machine_hours is not None:
            body["budgeted_machine_hours"] = _money("budgeted_machine_hours", budgeted_machine_hours)
        if budgeted_direct_labour_hours is not None:
            body["budgeted_direct_labour_hours"] = _money("budgeted_direct_labour_hours", budgeted_direct_labour_hours)
        if budgeted_direct_labour_cost is not None:
            body["budgeted_direct_labour_cost"] = _money("budgeted_direct_labour_cost", budgeted_direct_labour_cost)
        if budgeted_material_cost is not None:
            body["budgeted_material_cost"] = _money("budgeted_material_cost", budgeted_material_cost)
        if budgeted_prime_cost is not None:
            body["budgeted_prime_cost"] = _money("budgeted_prime_cost", budgeted_prime_cost)
        if budgeted_units is not None:
            body["budgeted_units"] = _money("budgeted_units", budgeted_units)
        query = None
        path = "/overhead-absorption/machine-hours-rate"
        return self._call("POST", path, query=query, body=body)

    def direct_labour_hours_rate(
        self,
        cost_centre_name: Any,
        budgeted_overhead: Any,
        budgeted_machine_hours: Optional[Any] = None,
        budgeted_direct_labour_hours: Optional[Any] = None,
        budgeted_direct_labour_cost: Optional[Any] = None,
        budgeted_material_cost: Optional[Any] = None,
        budgeted_prime_cost: Optional[Any] = None,
        budgeted_units: Optional[Any] = None,
    ) -> Any:
        """Absorption rate per direct labour hour."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if budgeted_overhead is not None:
            body["budgeted_overhead"] = _money("budgeted_overhead", budgeted_overhead)
        if budgeted_machine_hours is not None:
            body["budgeted_machine_hours"] = _money("budgeted_machine_hours", budgeted_machine_hours)
        if budgeted_direct_labour_hours is not None:
            body["budgeted_direct_labour_hours"] = _money("budgeted_direct_labour_hours", budgeted_direct_labour_hours)
        if budgeted_direct_labour_cost is not None:
            body["budgeted_direct_labour_cost"] = _money("budgeted_direct_labour_cost", budgeted_direct_labour_cost)
        if budgeted_material_cost is not None:
            body["budgeted_material_cost"] = _money("budgeted_material_cost", budgeted_material_cost)
        if budgeted_prime_cost is not None:
            body["budgeted_prime_cost"] = _money("budgeted_prime_cost", budgeted_prime_cost)
        if budgeted_units is not None:
            body["budgeted_units"] = _money("budgeted_units", budgeted_units)
        query = None
        path = "/overhead-absorption/labour-hours-rate"
        return self._call("POST", path, query=query, body=body)

    def direct_labour_cost_rate(
        self,
        cost_centre_name: Any,
        budgeted_overhead: Any,
        budgeted_machine_hours: Optional[Any] = None,
        budgeted_direct_labour_hours: Optional[Any] = None,
        budgeted_direct_labour_cost: Optional[Any] = None,
        budgeted_material_cost: Optional[Any] = None,
        budgeted_prime_cost: Optional[Any] = None,
        budgeted_units: Optional[Any] = None,
    ) -> Any:
        """Absorption rate as a percentage of direct labour cost."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if budgeted_overhead is not None:
            body["budgeted_overhead"] = _money("budgeted_overhead", budgeted_overhead)
        if budgeted_machine_hours is not None:
            body["budgeted_machine_hours"] = _money("budgeted_machine_hours", budgeted_machine_hours)
        if budgeted_direct_labour_hours is not None:
            body["budgeted_direct_labour_hours"] = _money("budgeted_direct_labour_hours", budgeted_direct_labour_hours)
        if budgeted_direct_labour_cost is not None:
            body["budgeted_direct_labour_cost"] = _money("budgeted_direct_labour_cost", budgeted_direct_labour_cost)
        if budgeted_material_cost is not None:
            body["budgeted_material_cost"] = _money("budgeted_material_cost", budgeted_material_cost)
        if budgeted_prime_cost is not None:
            body["budgeted_prime_cost"] = _money("budgeted_prime_cost", budgeted_prime_cost)
        if budgeted_units is not None:
            body["budgeted_units"] = _money("budgeted_units", budgeted_units)
        query = None
        path = "/overhead-absorption/labour-cost-rate"
        return self._call("POST", path, query=query, body=body)

    def material_cost_rate(
        self,
        cost_centre_name: Any,
        budgeted_overhead: Any,
        budgeted_machine_hours: Optional[Any] = None,
        budgeted_direct_labour_hours: Optional[Any] = None,
        budgeted_direct_labour_cost: Optional[Any] = None,
        budgeted_material_cost: Optional[Any] = None,
        budgeted_prime_cost: Optional[Any] = None,
        budgeted_units: Optional[Any] = None,
    ) -> Any:
        """Absorption rate as a percentage of material cost."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if budgeted_overhead is not None:
            body["budgeted_overhead"] = _money("budgeted_overhead", budgeted_overhead)
        if budgeted_machine_hours is not None:
            body["budgeted_machine_hours"] = _money("budgeted_machine_hours", budgeted_machine_hours)
        if budgeted_direct_labour_hours is not None:
            body["budgeted_direct_labour_hours"] = _money("budgeted_direct_labour_hours", budgeted_direct_labour_hours)
        if budgeted_direct_labour_cost is not None:
            body["budgeted_direct_labour_cost"] = _money("budgeted_direct_labour_cost", budgeted_direct_labour_cost)
        if budgeted_material_cost is not None:
            body["budgeted_material_cost"] = _money("budgeted_material_cost", budgeted_material_cost)
        if budgeted_prime_cost is not None:
            body["budgeted_prime_cost"] = _money("budgeted_prime_cost", budgeted_prime_cost)
        if budgeted_units is not None:
            body["budgeted_units"] = _money("budgeted_units", budgeted_units)
        query = None
        path = "/overhead-absorption/material-cost-rate"
        return self._call("POST", path, query=query, body=body)

    def prime_cost_rate(
        self,
        cost_centre_name: Any,
        budgeted_overhead: Any,
        budgeted_machine_hours: Optional[Any] = None,
        budgeted_direct_labour_hours: Optional[Any] = None,
        budgeted_direct_labour_cost: Optional[Any] = None,
        budgeted_material_cost: Optional[Any] = None,
        budgeted_prime_cost: Optional[Any] = None,
        budgeted_units: Optional[Any] = None,
    ) -> Any:
        """Absorption rate as a percentage of prime cost."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if budgeted_overhead is not None:
            body["budgeted_overhead"] = _money("budgeted_overhead", budgeted_overhead)
        if budgeted_machine_hours is not None:
            body["budgeted_machine_hours"] = _money("budgeted_machine_hours", budgeted_machine_hours)
        if budgeted_direct_labour_hours is not None:
            body["budgeted_direct_labour_hours"] = _money("budgeted_direct_labour_hours", budgeted_direct_labour_hours)
        if budgeted_direct_labour_cost is not None:
            body["budgeted_direct_labour_cost"] = _money("budgeted_direct_labour_cost", budgeted_direct_labour_cost)
        if budgeted_material_cost is not None:
            body["budgeted_material_cost"] = _money("budgeted_material_cost", budgeted_material_cost)
        if budgeted_prime_cost is not None:
            body["budgeted_prime_cost"] = _money("budgeted_prime_cost", budgeted_prime_cost)
        if budgeted_units is not None:
            body["budgeted_units"] = _money("budgeted_units", budgeted_units)
        query = None
        path = "/overhead-absorption/prime-cost-rate"
        return self._call("POST", path, query=query, body=body)

    def unit_rate(
        self,
        cost_centre_name: Any,
        budgeted_overhead: Any,
        budgeted_machine_hours: Optional[Any] = None,
        budgeted_direct_labour_hours: Optional[Any] = None,
        budgeted_direct_labour_cost: Optional[Any] = None,
        budgeted_material_cost: Optional[Any] = None,
        budgeted_prime_cost: Optional[Any] = None,
        budgeted_units: Optional[Any] = None,
    ) -> Any:
        """Absorption rate per unit produced."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if budgeted_overhead is not None:
            body["budgeted_overhead"] = _money("budgeted_overhead", budgeted_overhead)
        if budgeted_machine_hours is not None:
            body["budgeted_machine_hours"] = _money("budgeted_machine_hours", budgeted_machine_hours)
        if budgeted_direct_labour_hours is not None:
            body["budgeted_direct_labour_hours"] = _money("budgeted_direct_labour_hours", budgeted_direct_labour_hours)
        if budgeted_direct_labour_cost is not None:
            body["budgeted_direct_labour_cost"] = _money("budgeted_direct_labour_cost", budgeted_direct_labour_cost)
        if budgeted_material_cost is not None:
            body["budgeted_material_cost"] = _money("budgeted_material_cost", budgeted_material_cost)
        if budgeted_prime_cost is not None:
            body["budgeted_prime_cost"] = _money("budgeted_prime_cost", budgeted_prime_cost)
        if budgeted_units is not None:
            body["budgeted_units"] = _money("budgeted_units", budgeted_units)
        query = None
        path = "/overhead-absorption/unit-rate"
        return self._call("POST", path, query=query, body=body)

    def blanket_rate(
        self,
        entity_name: Any,
        absorption_base: Any,
        total_budgeted_overhead: Any,
        total_base_units: Any,
    ) -> Any:
        """One rate for the whole factory, from totals rather than a cost centre."""
        body: dict[str, Any] = {}
        if entity_name is not None:
            body["entity_name"] = entity_name
        if absorption_base is not None:
            body["absorption_base"] = absorption_base
        if total_budgeted_overhead is not None:
            body["total_budgeted_overhead"] = _money("total_budgeted_overhead", total_budgeted_overhead)
        if total_base_units is not None:
            body["total_base_units"] = _money("total_base_units", total_base_units)
        query = None
        path = "/overhead-absorption/blanket-rate"
        return self._call("POST", path, query=query, body=body)

    def absorbed_overhead(
        self,
        cost_centre_name: Any,
        actual_base_units: Any,
        absorption_rate: Any,
    ) -> Any:
        """What a rate recovers from the activity that actually happened."""
        body: dict[str, Any] = {}
        if cost_centre_name is not None:
            body["cost_centre_name"] = cost_centre_name
        if actual_base_units is not None:
            body["actual_base_units"] = _money("actual_base_units", actual_base_units)
        if absorption_rate is not None:
            body["absorption_rate"] = _money("absorption_rate", absorption_rate)
        query = None
        path = "/overhead-absorption/absorbed-overhead"
        return self._call("POST", path, query=query, body=body)

    def list_absorption_rates(
        self,
        cost_centre_id: Optional[str] = None,
    ) -> Any:
        """The rates recorded against this Book, newest the store's order."""
        query = {
            "cost_centre_id": cost_centre_id,
        }
        query = {key: value for key, value in query.items() if value is not None}
        path = "/overhead-absorption/rates"
        return self._call("GET", path, query=query, body=None)

    def list_absorption_analyses(
        self,
    ) -> Any:
        """Every rate calculation recorded against this Book."""
        query = None
        path = "/overhead-absorption/analyses"
        return self._call("GET", path, query=query, body=None)


__all__ = ["VimbaiOverheadAbsorptionClient", "VimbaiOverheadAbsorptionClientError"]
