"""Generated from `contracts/assets.yaml` by `scripts/generate_sdk.py`. Do not edit.

The asset register, and what a period of depreciation and a disposal do to it.

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


class VimbaiFixedAssetsRegisterClientError(Exception):
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


class VimbaiFixedAssetsRegisterClient:
    """The asset register, and what a period of depreciation and a disposal do to it.

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
            raise VimbaiFixedAssetsRegisterClientError(error.code, message) from None
        return json.loads(payload) if payload else None

    def register_fixed_asset(
        self,
        asset_code: Any,
        asset_name: Any,
        category: Any,
        acquisition_date: Any,
        acquisition_cost: Any,
        useful_life_years: Any,
        salvage_value: Optional[Any] = None,
        depreciation_method: Optional[Any] = None,
        location: Optional[Any] = None,
        department: Optional[Any] = None,
    ) -> Any:
        """Register a fixed asset."""
        body: dict[str, Any] = {}
        if asset_code is not None:
            body["asset_code"] = asset_code
        if asset_name is not None:
            body["asset_name"] = asset_name
        if category is not None:
            body["category"] = category
        if acquisition_date is not None:
            body["acquisition_date"] = acquisition_date
        if acquisition_cost is not None:
            body["acquisition_cost"] = _money("acquisition_cost", acquisition_cost)
        if useful_life_years is not None:
            body["useful_life_years"] = useful_life_years
        if salvage_value is not None:
            body["salvage_value"] = _money("salvage_value", salvage_value)
        if depreciation_method is not None:
            body["depreciation_method"] = depreciation_method
        if location is not None:
            body["location"] = location
        if department is not None:
            body["department"] = department
        query = None
        path = "/fixed-assets-register/assets"
        return self._call("POST", path, query=query, body=body)

    def list_fixed_assets(
        self,
        category: Optional[str] = None,
        status: Optional[str] = None,
        department: Optional[str] = None,
    ) -> Any:
        """The caller's register, optionally narrowed."""
        query = {
            "category": category,
            "status": status,
            "department": department,
        }
        query = {key: value for key, value in query.items() if value is not None}
        path = "/fixed-assets-register/assets"
        return self._call("GET", path, query=query, body=None)

    def get_fixed_asset(
        self,
        asset_id: str,
    ) -> Any:
        """One asset from the caller's register."""
        query = None
        path = f"/fixed-assets-register/assets/{asset_id}"
        return self._call("GET", path, query=query, body=None)

    def depreciate_fixed_asset(
        self,
        asset_id: str,
        period: str,
    ) -> Any:
        """Charge one period of depreciation and record the entry."""
        query = {
            "period": period,
        }
        query = {key: value for key, value in query.items() if value is not None}
        path = f"/fixed-assets-register/assets/{asset_id}/depreciate"
        return self._call("POST", path, query=query, body=None)

    def list_depreciation(
        self,
        asset_id: str,
    ) -> Any:
        """The entries charged against one asset."""
        query = None
        path = f"/fixed-assets-register/assets/{asset_id}/depreciation"
        return self._call("GET", path, query=query, body=None)

    def dispose_fixed_asset(
        self,
        asset_id: str,
        disposal_date: str,
        disposal_value: Optional[str] = None,
        disposal_method: Optional[str] = None,
        notes: Optional[str] = None,
    ) -> Any:
        """Dispose of an asset and record the gain or loss."""
        query = {
            "disposal_date": disposal_date,
            "disposal_value": disposal_value,
            "disposal_method": disposal_method,
            "notes": notes,
        }
        query = {key: value for key, value in query.items() if value is not None}
        path = f"/fixed-assets-register/assets/{asset_id}/dispose"
        return self._call("POST", path, query=query, body=None)

    def fixed_asset_summary(
        self,
    ) -> Any:
        """The register totals, and the net book value held per category."""
        query = None
        path = "/fixed-assets-register/summary"
        return self._call("GET", path, query=query, body=None)


__all__ = ["VimbaiFixedAssetsRegisterClient", "VimbaiFixedAssetsRegisterClientError"]
