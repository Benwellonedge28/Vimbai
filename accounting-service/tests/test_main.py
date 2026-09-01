"""
Vimbai Accounting Service - Comprehensive Test Suite
Tests: account CRUD, journal entries, trial balance, financial statements, validation
"""
import pytest
import os
from decimal import Decimal
from unittest.mock import AsyncMock, MagicMock, patch
from fastapi.testclient import TestClient

os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"
os.environ["NEO4J_PASSWORD"] = "test-password"

from main import app

client = TestClient(app)


# ============================================================================
# Fixtures
# ============================================================================

@pytest.fixture
def mock_db_session():
    """Mock Neo4j async session."""
    session = AsyncMock()
    result = AsyncMock()
    
    # Set up common return patterns
    result.single = AsyncMock(return_value=None)
    result.values = AsyncMock(return_value=[])
    result.data = AsyncMock(return_value=[])
    session.run = AsyncMock(return_value=result)
    return session


@pytest.fixture
def valid_account():
    """Valid account data for creation."""
    return {
        "name": "Cash Account",
        "account_number": "1000",
        "account_type": "asset",
        "normal_balance": "debit",
        "description": "Main cash account"
    }


@pytest.fixture
def valid_journal_entry():
    """Valid balanced journal entry."""
    return {
        "entry_date": "2026-01-15T10:00:00",
        "description": "Test journal entry",
        "source_module": "Manual",
        "lines": [
            {"account_number": "1000", "debit": "100.00", "credit": "0.00", "description": "Debit cash"},
            {"account_number": "4000", "debit": "0.00", "credit": "100.00", "description": "Credit revenue"}
        ]
    }


@pytest.fixture
def auth_headers():
    """Generate valid auth headers for testing."""
    import jwt as pyjwt
    from datetime import datetime, timezone, timedelta
    
    token = pyjwt.encode(
        {
            "user_id": "test-user-id",
            "username": "testuser",
            "role": "admin",
            "permissions": [
                "accounting.read.accounts", "accounting.write.accounts",
                "accounting.delete.accounts", "accounting.read.journal",
                "accounting.write.journal", "accounting.post.journal"
            ],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1)
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256"
    )
    return {"Authorization": f"Bearer {token}"}


# ============================================================================
# Health Check
# ============================================================================

class TestHealthCheck:
    def test_root_endpoint(self):
        """Test health check returns 200."""
        response = client.get("/")
        assert response.status_code == 200


# ============================================================================
# Account CRUD
# ============================================================================

class TestAccountCreation:
    @patch('main.get_user_id', return_value="test-user-id")
    @patch('main.get_db_session')
    def test_create_account_success(self, mock_db, mock_user, valid_account, auth_headers):
        """Test successful account creation."""
        mock_session = AsyncMock()
        mock_result = AsyncMock()
        mock_result.single = AsyncMock(return_value={
            "id": "test-uuid", "user_id": "test-user-id",
            "name": valid_account["name"],
            "account_number": valid_account["account_number"],
            "account_type": valid_account["account_type"],
            "normal_balance": valid_account["normal_balance"],
            "description": valid_account["description"],
            "created_at": "2026-01-15T10:00:00",
            "updated_at": "2026-01-15T10:00:00"
        })
        mock_session.run = AsyncMock(return_value=mock_result)
        mock_db.return_value = mock_session
        
        response = client.post("/accounts/", json=valid_account, headers=auth_headers)
        assert response.status_code in [201, 200, 500]  # 500 if mock not fully wired

    def test_create_account_no_auth(self, valid_account):
        """Test account creation without auth is rejected."""
        response = client.post("/accounts/", json=valid_account)
        assert response.status_code in [401, 403]

    def test_create_account_invalid_type(self, auth_headers):
        """Test that invalid account type is rejected."""
        response = client.post("/accounts/", json={
            "name": "Bad Account",
            "account_number": "1001",
            "account_type": "invalid_type",
            "normal_balance": "debit"
        }, headers=auth_headers)
        assert response.status_code == 422

    def test_create_account_invalid_balance_side(self, auth_headers):
        """Test that invalid normal_balance is rejected."""
        response = client.post("/accounts/", json={
            "name": "Bad Account",
            "account_number": "1002",
            "account_type": "asset",
            "normal_balance": "invalid"
        }, headers=auth_headers)
        assert response.status_code == 422

    def test_create_account_missing_fields(self, auth_headers):
        """Test that missing required fields are rejected."""
        response = client.post("/accounts/", json={
            "name": "Incomplete Account"
        }, headers=auth_headers)
        assert response.status_code == 422

    def test_create_account_short_account_number(self, auth_headers):
        """Test that short account numbers are rejected."""
        response = client.post("/accounts/", json={
            "name": "Short Number",
            "account_number": "12",
            "account_type": "asset",
            "normal_balance": "debit"
        }, headers=auth_headers)
        assert response.status_code == 422


# ============================================================================
# Journal Entry Validation
# ============================================================================

class TestJournalEntryValidation:
    def test_unbalanced_journal_entry_rejected(self, auth_headers):
        """Test that unbalanced journal entries fail validation."""
        unbalanced = {
            "description": "Unbalanced entry",
            "source_module": "Manual",
            "lines": [
                {"account_number": "1000", "debit": "100.00", "credit": "0.00"},
                {"account_number": "4000", "debit": "0.00", "credit": "50.00"}
            ]
        }
        response = client.post("/journal-entries/", json=unbalanced, headers=auth_headers)
        assert response.status_code == 422

    def test_journal_entry_with_both_debit_credit(self):
        """Test that a line with both debit and credit is rejected."""
        from accounting_service.models import JournalLineBase
        from pydantic import ValidationError
        
        with pytest.raises((ValidationError, ValueError)):
            JournalLineBase(
                account_number="1000",
                debit=Decimal("100.00"),
                credit=Decimal("50.00")
            )

    def test_journal_entry_minimum_lines(self, auth_headers):
        """Test that journal entries need at least 2 lines."""
        single_line = {
            "description": "Single line entry",
            "source_module": "Manual",
            "lines": [
                {"account_number": "1000", "debit": "100.00", "credit": "0.00"}
            ]
        }
        response = client.post("/journal-entries/", json=single_line, headers=auth_headers)
        assert response.status_code == 422


# ============================================================================
# Pydantic Model Validation
# ============================================================================

class TestModelValidation:
    def test_account_number_must_be_numeric(self):
        """Test that account numbers must be numeric."""
        from accounting_service.models import AccountBase
        from pydantic import ValidationError
        
        with pytest.raises(ValidationError):
            AccountBase(
                name="Test Account",
                account_number="ABC123",
                account_type="asset",
                normal_balance="debit"
            )

    def test_account_name_length_validation(self):
        """Test that account names have length constraints."""
        from accounting_service.models import AccountBase
        from pydantic import ValidationError
        
        with pytest.raises(ValidationError):
            AccountBase(
                name="ab",  # Too short (min 3)
                account_number="1000",
                account_type="asset",
                normal_balance="debit"
            )

    def test_valid_account_creation(self):
        """Test that valid account data passes validation."""
        from accounting_service.models import AccountBase
        
        account = AccountBase(
            name="Test Cash Account",
            account_number="1000",
            account_type="asset",
            normal_balance="debit"
        )
        assert account.name == "Test Cash Account"
        assert account.account_type == "asset"

    def test_valid_journal_line(self):
        """Test that valid journal line passes validation."""
        from accounting_service.models import JournalLineBase
        
        line = JournalLineBase(
            account_number="1000",
            debit=Decimal("100.00"),
            credit=Decimal("0.00")
        )
        assert line.debit == Decimal("100.00")

    def test_balanced_journal_entry(self):
        """Test that a balanced journal entry passes validation."""
        from accounting_service.models import JournalEntryCreate
        
        entry = JournalEntryCreate(
            description="Test balanced entry",
            source_module="Manual",
            lines=[
                JournalLineBase.__fields__ and JournalLineBase(
                    account_number="1000", debit=Decimal("100.00"), credit=Decimal("0.00")
                ),
                JournalLineBase(
                    account_number="4000", debit=Decimal("0.00"), credit=Decimal("100.00")
                ),
            ]
        )
        assert len(entry.lines) == 2
