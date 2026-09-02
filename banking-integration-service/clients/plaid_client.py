import os
from typing import Any, Dict, List, Optional

import httpx
from dotenv import load_dotenv

load_dotenv()


class PlaidClientException(Exception):
    """Custom exception for Plaid API errors."""

    def __init__(self, message: str, status_code: Optional[int] = None, error_code: Optional[str] = None):
        super().__init__(message)
        self.status_code = status_code
        self.error_code = error_code


class PlaidClient:
    def __init__(self):
        self.client_id = os.getenv("PLAID_CLIENT_ID")
        self.secret = os.getenv("PLAID_SECRET")
        self.env = os.getenv("PLAID_ENV", "sandbox")  # sandbox, development, production

        if not self.client_id or not self.secret:
            raise PlaidClientException("PLAID_CLIENT_ID and PLAID_SECRET environment variables must be set.")

        self.base_url = self._get_base_url()
        self.headers = {
            "Content-Type": "application/json",
        }

    def _get_base_url(self) -> str:
        if self.env == "sandbox":
            return "https://sandbox.plaid.com"
        elif self.env == "development":
            return "https://development.plaid.com"
        elif self.env == "production":
            return "https://api.plaid.com"
        else:
            raise PlaidClientException(f"Invalid PLAID_ENV: {self.env}")

    async def _post(self, path: str, data: Dict[str, Any]) -> Dict[str, Any]:
        full_url = f"{self.base_url}{path}"
        payload = {"client_id": self.client_id, "secret": self.secret, **data}
        async with httpx.AsyncClient() as client:
            response = await client.post(full_url, headers=self.headers, json=payload)
            response_json = response.json()

            if response.status_code != 200:
                error_message = response_json.get("error_message", "Unknown Plaid API error")
                error_code = response_json.get("error_code")
                raise PlaidClientException(error_message, response.status_code, error_code)
            return response_json

    async def create_link_token(self, user_id: str, client_name: str) -> Dict[str, Any]:
        """Create a link token for Plaid Link initialization."""
        return await self._post(
            "/link/token/create",
            {
                "user": {"client_user_id": user_id},
                "client_name": client_name,
                "products": ["transactions"],  # We are interested in transactions
                "country_codes": ["US"],  # Example country code
                "language": "en",
                "webhook": "https://your-webhook-url.com/plaid/webhook",  # Placeholder webhook URL
                "redirect_uri": "https://your-redirect-uri.com/plaid/oauth",  # Optional for OAuth flows
                "account_filters": {  # Optional: filter accounts by type
                    "depository": {"account_subtypes": ["checking", "savings"]},
                    "credit": {"account_subtypes": ["credit card"]},
                },
            },
        )

    async def exchange_public_token(self, public_token: str) -> Dict[str, Any]:
        """Exchange a public token for an access token."""
        return await self._post("/item/public_token/exchange", {"public_token": public_token})

    async def get_accounts(self, access_token: str) -> Dict[str, Any]:
        """Fetch accounts associated with an access token."""
        return await self._post("/accounts/get", {"access_token": access_token})

    async def get_transactions(
        self, access_token: str, start_date: str, end_date: str, options: Optional[Dict[str, Any]] = None
    ) -> Dict[str, Any]:
        """Fetch transactions for an item."""
        return await self._post(
            "/transactions/get",
            {"access_token": access_token, "start_date": start_date, "end_date": end_date, "options": options},
        )

    async def get_item(self, access_token: str) -> Dict[str, Any]:
        """Retrieve information about an item."""
        return await self._post("/item/get", {"access_token": access_token})

    async def remove_item(self, access_token: str) -> Dict[str, Any]:
        """Invalidate a Plaid Item. This is irreversible."""
        return await self._post("/item/remove", {"access_token": access_token})

    async def revoke_access_token(self, access_token: str) -> Dict[str, Any]:
        """Revoke an access token."""
        return await self._post(
            "/item/remove",  # Plaid uses item/remove for revoking. There is also /item/access_token/invalidate
            {"access_token": access_token},
        )
