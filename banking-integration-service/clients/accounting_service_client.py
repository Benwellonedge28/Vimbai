import os
from typing import Any, Dict, List, Optional

import httpx
from dotenv import load_dotenv

# Assuming accounting_service.models.py is available (e.g., copied or shared module)
# For a proper microservices setup, this would be imported from a shared client library
# or re-defined. For simplicity, we'll assume the structure of JournalEntryCreate.
# In a real system, you'd define this in a common 'shared_models' or similar.


# Placeholder for JournalEntryCreate/JournalLineCreate from accounting_service
# In a production environment, these would be imported from a shared library/package
class MockJournalLineCreate:
    def __init__(self, account_number: str, debit: float, credit: float, description: str):
        self.account_number = account_number
        self.debit = debit
        self.credit = credit
        self.description = description

    def dict(self):
        return {
            "account_number": self.account_number,
            "debit": float(self.debit),
            "credit": float(self.credit),
            "description": self.description,
        }


class MockJournalEntryCreate:
    def __init__(
        self,
        entry_date: str,
        description: str,
        source_module: str,
        reference_number: str,
        lines: List[MockJournalLineCreate],
    ):
        self.entry_date = entry_date
        self.description = description
        self.source_module = source_module
        self.reference_number = reference_number
        self.lines = lines

    def json(self):
        return {
            "entry_date": self.entry_date,
            "description": self.description,
            "source_module": self.source_module,
            "reference_number": self.reference_number,
            "lines": [line.dict() for line in self.lines],
        }


load_dotenv()


class AccountingServiceClientException(Exception):
    """Custom exception for Accounting Service API errors."""

    def __init__(self, message: str, status_code: Optional[int] = None):
        super().__init__(message)
        self.status_code = status_code


class AccountingServiceClient:
    def __init__(self, base_url: str = None):
        self.base_url = base_url or os.getenv("ACCOUNTING_SERVICE_URL")
        if not self.base_url:
            raise AccountingServiceClientException("ACCOUNTING_SERVICE_URL environment variable must be set.")

    async def create_journal_entry(self, journal_entry_data: MockJournalEntryCreate, user_id: str) -> Dict[str, Any]:
        """Calls the Accounting Service to create a new Journal Entry."""
        url = f"{self.base_url}/journal-entries/"
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {user_id}",  # In a real system, this would be a valid JWT or internal service token
        }
        async with httpx.AsyncClient() as client:
            response = await client.post(url, headers=headers, json=journal_entry_data.json())

            if response.status_code != 201:
                error_detail = response.json().get("detail", response.text)
                raise AccountingServiceClientException(
                    f"Failed to create Journal Entry: {error_detail}", response.status_code
                )
            return response.json()

    async def get_chart_of_accounts(self, user_id: str) -> List[Dict[str, Any]]:
        """Calls the Accounting Service to get the Chart of Accounts."""
        url = f"{self.base_url}/accounts/"
        headers = {
            "Authorization": f"Bearer {user_id}",  # Use user_id as a mock token
        }
        async with httpx.AsyncClient() as client:
            response = await client.get(url, headers=headers)
            if response.status_code != 200:
                error_detail = response.json().get("detail", response.text)
                raise AccountingServiceClientException(
                    f"Failed to fetch Chart of Accounts: {error_detail}", response.status_code
                )
            return response.json()

    # Add other Accounting Service methods as needed (e.g., get_account_by_number, get_journal_entry_by_id)
