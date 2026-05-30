"""
Integration tests for Banking Integration Service
Tests bank account management and transaction reconciliation
"""
import pytest
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from fastapi.testclient import TestClient
from banking_integration_service.main import app

client = TestClient(app)

@pytest.fixture
def auth_headers():
    """Get authentication headers for API calls"""
    response = client.post("/auth/login", json={
        "username": "test_user",
        "password": "test_password"
    })
    token = response.json().get("access_token")
    return {"Authorization": f"Bearer {token}"}

def test_health_check():
    """Test the health check endpoint"""
    response = client.get("/")
    assert response.status_code == 200
    assert "Banking Integration Service" in response.json()["message"]

def test_get_bank_accounts():
    """Test retrieving all bank accounts"""
    response = client.get("/banking/accounts")
    assert response.status_code == 200
    data = response.json()
    assert isinstance(data, list)

def test_create_bank_account(auth_headers):
    """Test creating a new bank account"""
    account_data = {
        "bank_name": "Test Bank",
        "account_name": "Test Checking Account",
        "account_id": "ACC-001",
        "account_type": "checking",
        "currency": "USD",
        "current_balance": 10000.00
    }
    response = client.post(
        "/banking/accounts",
        json=account_data,
        headers=auth_headers
    )
    assert response.status_code == 201
    data = response.json()
    assert data["bank_name"] == "Test Bank"
    assert data["current_balance"] == 10000.00

def test_sync_bank_account(auth_headers):
    """Test syncing a bank account with external service"""
    # First create an account
    account_data = {
        "bank_name": "Sync Test Bank",
        "account_name": "Sync Checking",
        "account_id": "SYNC-001",
        "account_type": "savings",
        "currency": "USD",
        "current_balance": 5000.00
    }
    create_response = client.post(
        "/banking/accounts",
        json=account_data,
        headers=auth_headers
    )
    account_id = create_response.json()["id"]

    # Sync the account
    sync_response = client.post(
        f"/banking/accounts/{account_id}/sync",
        headers=auth_headers
    )
    assert sync_response.status_code == 200

def test_get_transactions_for_account(auth_headers):
    """Test retrieving transactions for a specific account"""
    # Create account first
    account_data = {
        "bank_name": "Transaction Test Bank",
        "account_name": "Transaction Account",
        "account_id": "TXN-001",
        "account_type": "checking",
        "currency": "USD",
        "current_balance": 1000.00
    }
    create_response = client.post(
        "/banking/accounts",
        json=account_data,
        headers=auth_headers
    )
    account_id = create_response.json()["id"]

    # Get transactions
    response = client.get(f"/banking/accounts/{account_id}/transactions")
    assert response.status_code == 200
    data = response.json()
    assert isinstance(data, list)

def test_reconcile_transaction(auth_headers):
    """Test reconciling a bank transaction"""
    # Create account and transaction
    account_data = {
        "bank_name": "Reconcile Test Bank",
        "account_name": "Reconcile Account",
        "account_id": "REC-001",
        "account_type": "checking",
        "currency": "USD",
        "current_balance": 5000.00
    }
    create_response = client.post(
        "/banking/accounts",
        json=account_data,
        headers=auth_headers
    )
    account_id = create_response.json()["id"]

    # Create a transaction
    transaction_data = {
        "transaction_id": "TXN-TEST-001",
        "date": "2026-05-15",
        "description": "Test transaction",
        "amount": 100.00,
        "transaction_type": "debit"
    }
    txn_response = client.post(
        f"/banking/accounts/{account_id}/transactions",
        json=transaction_data,
        headers=auth_headers
    )
    transaction_id = txn_response.json()["id"]

    # Reconcile the transaction
    reconcile_response = client.post(
        f"/banking/transactions/{transaction_id}/reconcile",
        headers=auth_headers
    )
    assert reconcile_response.status_code == 200

def test_manual_bank_entry(auth_headers):
    """Test creating a manual bank entry"""
    entry_data = {
        "description": "Manual deposit",
        "account_number": "1001",
        "debit": 0,
        "credit": 500.00,
        "reference_number": "MAN-001"
    }
    response = client.post(
        "/banking/journal-entries",
        json=entry_data,
        headers=auth_headers
    )
    assert response.status_code == 201

def test_bank_reconciliation_report(auth_headers):
    """Test generating a bank reconciliation report"""
    response = client.get(
        "/banking/reconciliation/report",
        params={"account_id": "test-account", "start_date": "2026-01-01", "end_date": "2026-05-29"},
        headers=auth_headers
    )
    assert response.status_code == 200
    data = response.json()
    assert "account_id" in data
    assert "book_balance" in data
    assert "bank_balance" in data
    assert "reconciled_items" in data

def test_auto_reconcile(auth_headers):
    """Test automatic reconciliation of transactions"""
    response = client.post(
        "/banking/reconciliation/auto",
        params={"account_id": "test-account"},
        headers=auth_headers
    )
    assert response.status_code == 200
    data = response.json()
    assert "matched_items" in data
    assert "unmatched_items" in data

def test_rule_based_matching(auth_headers):
    """Test rule-based transaction matching"""
    rules = [
        {"field": "description", "operator": "contains", "value": "PAYROLL"},
        {"field": "amount", "operator": "equals", "value": 5000}
    ]
    response = client.post(
        "/banking/matching/apply-rules",
        json=rules,
        headers=auth_headers
    )
    assert response.status_code == 200

def test_validation_errors():
    """Test validation of invalid bank account data"""
    invalid_data = {
        "bank_name": "",  # Empty name should fail
        "account_name": "Test",
        "account_id": "ACC",
        "account_type": "invalid_type",
        "currency": "INVALID",
        "current_balance": -100  # Negative balance
    }
    response = client.post(
        "/banking/accounts",
        json=invalid_data
    )
    # Should return validation error
    assert response.status_code in [400, 422]

if __name__ == "__main__":
    pytest.main([__file__, "-v"])