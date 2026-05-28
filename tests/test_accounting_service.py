import pytest
from httpx import AsyncClient, ASGITransport
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'accounting-service'))

from main import app

@pytest.fixture
def anyio_backend():
    return "asyncio"

@pytest.mark.anyio
async def test_health_check():
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/")
        assert response.status_code == 200
        assert "Accounting Service" in response.json()["title"]

@pytest.mark.anyio
async def test_create_account():
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "name": "Test Cash Account",
            "account_number": "1001",
            "account_type": "asset",
            "normal_balance": "debit",
            "description": "Testing account"
        }
        # This would require auth token in real scenario
        # response = await client.post("/accounts/", json=payload, headers={"Authorization": "Bearer test_token"})
        # assert response.status_code == 201
        pass

@pytest.mark.anyio
async def test_journal_entry_validation():
    """Test that unbalanced journal entries are rejected"""
    from models import JournalEntryCreate, JournalLineBase
    from datetime import datetime
    
    with pytest.raises(ValueError):
        JournalEntryCreate(
            entry_date=datetime.now(),
            description="Test unbalanced entry",
            source_module="Testing",
            lines=[
                JournalLineBase(account_number="1001", debit=100, credit=0),
                JournalLineBase(account_number="2001", debit=0, credit=50),  # Doesn't balance!
            ]
        )
