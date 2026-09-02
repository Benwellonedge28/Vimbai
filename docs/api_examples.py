"""
API Usage Examples and Documentation for Vimbai

This module provides comprehensive examples demonstrating how to use
the Vimbai API endpoints effectively.

Topics Covered:
1. Authentication
2. Chart of Accounts
3. Journal Entries
4. Financial Reports
5. NPO Fund Accounting
6. Error Handling
7. Batch Operations

Usage:
    python api_examples.py
"""

import json
from datetime import date, datetime, timedelta
from decimal import Decimal
from typing import Dict, List, Optional

import requests

# =============================================================================
# CONFIGURATION
# =============================================================================

# Base URL for Vimbai API
BASE_URL = "http://localhost:8000"

# Authentication credentials
USERNAME = "demo_user"
PASSWORD = "demo_password"

# Global headers (to be updated after authentication)
HEADERS = {"Content-Type": "application/json", "Accept": "application/json"}


# =============================================================================
# AUTHENTICATION EXAMPLES
# =============================================================================


def authenticate() -> Optional[str]:
    """
    Authenticate with Vimbai and get JWT token.

    Returns:
        JWT token string or None if authentication fails

    Example:
        >>> token = authenticate()
        >>> print(f"Token: {token}")
    """
    print("\n=== Authentication ===")

    # Login endpoint
    login_url = f"{BASE_URL}/auth/login"

    credentials = {"username": USERNAME, "password": PASSWORD}

    try:
        response = requests.post(login_url, json=credentials, headers=HEADERS)

        if response.status_code == 200:
            data = response.json()
            token = data.get("access_token")
            HEADERS["Authorization"] = f"Bearer {token}"
            print(f"Authentication successful")
            print(f"Token: {token[:20]}...")
            return token
        else:
            print(f"Authentication failed: {response.status_code}")
            print(f"Response: {response.text}")
            return None

    except requests.exceptions.ConnectionError:
        print(f"Cannot connect to {BASE_URL}. Is the server running?")
        return None


# =============================================================================
# CHART OF ACCOUNTS EXAMPLES
# =============================================================================


def create_account(account_number: str, account_name: str, account_type: str) -> Dict:
    """
    Create a new account in the Chart of Accounts.

    Args:
        account_number: Unique account identifier (e.g., "ACC-1001")
        account_name: Human-readable name
        account_type: Type of account (Asset, Liability, Equity, Revenue, Expense)

    Returns:
        Created account dictionary

    Example:
        >>> account = create_account("ACC-1001", "Cash", "Asset")
        >>> print(f"Created: {account['account_number']}")
    """
    print(f"\n=== Creating Account: {account_number} ===")

    url = f"{BASE_URL}/accounts/"
    data = {
        "account_number": account_number,
        "account_name": account_name,
        "account_type": account_type,
        "description": f"Created via API at {datetime.now()}",
        "is_active": True,
        "is_control_account": False,
        "parent_account_number": None,
    }

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        account = response.json()
        print(f"Success: Created {account['account_number']} - {account['account_name']}")
        return account
    else:
        print(f"Failed: {response.status_code}")
        print(f"Response: {response.text}")
        return {}


def get_account(account_number: str) -> Dict:
    """
    Retrieve account details by account number.

    Args:
        account_number: Account identifier

    Returns:
        Account dictionary
    """
    print(f"\n=== Getting Account: {account_number} ===")

    url = f"{BASE_URL}/accounts/{account_number}"
    response = requests.get(url, headers=HEADERS)

    if response.status_code == 200:
        return response.json()
    elif response.status_code == 404:
        print("Account not found")
        return {}
    else:
        print(f"Error: {response.status_code}")
        return {}


def list_accounts(account_type: Optional[str] = None) -> List[Dict]:
    """
    List all accounts, optionally filtered by type.

    Args:
        account_type: Filter by account type (optional)

    Returns:
        List of account dictionaries
    """
    print(f"\n=== Listing Accounts (type={account_type or 'all'}) ===")

    url = f"{BASE_URL}/accounts/"
    if account_type:
        url += f"?account_type={account_type}"

    response = requests.get(url, headers=HEADERS)

    if response.status_code == 200:
        accounts = response.json()
        print(f"Found {len(accounts)} accounts")
        return accounts
    else:
        print(f"Error: {response.status_code}")
        return []


def update_account(account_number: str, updates: Dict) -> Dict:
    """
    Update account properties.

    Args:
        account_number: Account to update
        updates: Dictionary of fields to update

    Returns:
        Updated account dictionary
    """
    print(f"\n=== Updating Account: {account_number} ===")

    url = f"{BASE_URL}/accounts/{account_number}"
    response = requests.put(url, json=updates, headers=HEADERS)

    if response.status_code == 200:
        print(f"Success: Updated {account_number}")
        return response.json()
    else:
        print(f"Failed: {response.status_code}")
        return {}


# =============================================================================
# JOURNAL ENTRIES EXAMPLES
# =============================================================================


def create_journal_entry(
    entry_date: str, description: str, reference: str, debit_account: str, credit_account: str, amount: Decimal
) -> Dict:
    """
    Create a double-entry journal entry.

    Args:
        entry_date: Date of entry (ISO format)
        description: Entry description
        reference: Reference/document number
        debit_account: Account to debit
        credit_account: Account to credit
        amount: Transaction amount

    Returns:
        Created journal entry dictionary

    Example:
        >>> entry = create_journal_entry(
        ...     "2024-01-15",
        ...     "Cash received from customer",
        ...     "INV-001",
        ...     "ACC-1001",  # Cash
        ...     "ACC-4001",  # Revenue
        ...     Decimal("1000.00")
        ... )
    """
    print(f"\n=== Creating Journal Entry: {reference} ===")

    url = f"{BASE_URL}/journal-entries/"
    data = {
        "entry_date": entry_date,
        "description": description,
        "reference_number": reference,
        "entries": [
            {
                "account_number": debit_account,
                "debit_amount": str(amount),
                "credit_amount": "0",
                "description": f"Debit: {description}",
            },
            {
                "account_number": credit_account,
                "debit_amount": "0",
                "credit_amount": str(amount),
                "description": f"Credit: {description}",
            },
        ],
    }

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        entry = response.json()
        print(f"Success: Entry {entry['entry_id']} created")
        print(f"  Amount: {amount}")
        print(f"  Debit: {debit_account}")
        print(f"  Credit: {credit_account}")
        return entry
    else:
        print(f"Failed: {response.status_code}")
        print(f"Response: {response.text}")
        return {}


def get_journal_entries(start_date: str, end_date: str) -> List[Dict]:
    """
    Retrieve journal entries within a date range.

    Args:
        start_date: Start date (ISO format)
        end_date: End date (ISO format)

    Returns:
        List of journal entries
    """
    print(f"\n=== Journal Entries: {start_date} to {end_date} ===")

    url = f"{BASE_URL}/journal-entries/?start_date={start_date}&end_date={end_date}"
    response = requests.get(url, headers=HEADERS)

    if response.status_code == 200:
        entries = response.json()
        print(f"Found {len(entries)} entries")
        return entries
    else:
        print(f"Error: {response.status_code}")
        return []


# =============================================================================
# FINANCIAL REPORTS EXAMPLES
# =============================================================================


def get_trial_balance(as_of_date: Optional[str] = None) -> Dict:
    """
    Generate trial balance report.

    Args:
        as_of_date: Date for trial balance (optional, defaults to today)

    Returns:
        Trial balance dictionary with debit/credit columns
    """
    print("\n=== Trial Balance ===")

    url = f"{BASE_URL}/trial-balance/"
    if as_of_date:
        url += f"?as_of_date={as_of_date}"

    response = requests.get(url, headers=HEADERS)

    if response.status_code == 200:
        tb = response.json()
        print(f"As of: {tb.get('as_of_date')}")
        print(f"Total Debits: {tb.get('total_debits')}")
        print(f"Total Credits: {tb.get('total_credits')}")
        return tb
    else:
        print(f"Error: {response.status_code}")
        return {}


def get_income_statement(start_date: str, end_date: str) -> Dict:
    """
    Generate income statement (Profit & Loss) report.

    Args:
        start_date: Period start date
        end_date: Period end date

    Returns:
        Income statement dictionary
    """
    print(f"\n=== Income Statement: {start_date} to {end_date} ===")

    url = f"{BASE_URL}/income-statement/?start_date={start_date}&end_date={end_date}"
    response = requests.get(url, headers=HEADERS)

    if response.status_code == 200:
        stmt = response.json()
        print(f"Revenue: {stmt.get('total_revenue', 0)}")
        print(f"Expenses: {stmt.get('total_expenses', 0)}")
        print(f"Net Income: {stmt.get('net_income', 0)}")
        return stmt
    else:
        print(f"Error: {response.status_code}")
        return {}


def get_balance_sheet(as_of_date: str) -> Dict:
    """
    Generate balance sheet report.

    Args:
        as_of_date: Date for balance sheet

    Returns:
        Balance sheet dictionary
    """
    print(f"\n=== Balance Sheet: {as_of_date} ===")

    url = f"{BASE_URL}/balance-sheet/?as_of_date={as_of_date}"
    response = requests.get(url, headers=HEADERS)

    if response.status_code == 200:
        bs = response.json()
        print(f"Total Assets: {bs.get('total_assets', 0)}")
        print(f"Total Liabilities: {bs.get('total_liabilities', 0)}")
        print(f"Total Equity: {bs.get('total_equity', 0)}")
        return bs
    else:
        print(f"Error: {response.status_code}")
        return {}


# =============================================================================
# NPO FUND ACCOUNTING EXAMPLES
# =============================================================================


def create_npo_fund(fund_name: str, fund_code: str, fund_type: str, description: str = "") -> Dict:
    """
    Create a new NPO fund.

    Args:
        fund_name: Fund name
        fund_code: Unique fund code
        fund_type: Type (general, restricted, endowment, capital, project)
        description: Fund description

    Returns:
        Created fund dictionary

    Example:
        >>> fund = create_npo_fund(
        ...     "Building Fund",
        ...     "FUND-001",
        ...     "restricted",
        ...     "For building improvements"
        ... )
    """
    print(f"\n=== Creating NPO Fund: {fund_code} ===")

    url = f"{BASE_URL}/funds/"
    data = {
        "fund_name": fund_name,
        "fund_code": fund_code,
        "fund_type": fund_type,
        "description": description,
        "current_balance": "0.00",
        "total_contributions": "0.00",
        "total_disbursements": "0.00",
    }

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        fund = response.json()
        print(f"Success: Created {fund['fund_code']} - {fund['fund_name']}")
        return fund
    else:
        print(f"Failed: {response.status_code}")
        return {}


def create_donation(
    donor_id: str, amount: Decimal, donation_type: str, fund_id: str, payment_method: str = "cash"
) -> Dict:
    """
    Record a donation.

    Args:
        donor_id: Donor identifier
        amount: Donation amount
        donation_type: Type (one_time, recurring, matching, in_kind)
        fund_id: Fund to credit
        payment_method: Payment method

    Returns:
        Created donation dictionary
    """
    print(f"\n=== Recording Donation from {donor_id} ===")

    url = f"{BASE_URL}/donations/"
    data = {
        "donor_id": donor_id,
        "donation_date": datetime.now().isoformat(),
        "amount": str(amount),
        "donation_type": donation_type,
        "designation": "general",
        "payment_method": payment_method,
        "fund_id": fund_id,
        "receipt_number": f"RCP-{datetime.now().strftime('%Y%m%d%H%M%S')}",
    }

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        donation = response.json()
        print(f"Success: Donation {donation.get('id')} recorded")
        print(f"  Amount: {amount}")
        print(f"  Type: {donation_type}")
        return donation
    else:
        print(f"Failed: {response.status_code}")
        return {}


def create_grant(grant_name: str, grantor: str, amount: Decimal, start_date: str, end_date: str, fund_id: str) -> Dict:
    """
    Create a new grant.

    Args:
        grant_name: Grant name
        grantor: Funding organization
        amount: Grant amount
        start_date: Grant start date
        end_date: Grant end date
        fund_id: Associated fund

    Returns:
        Created grant dictionary
    """
    print(f"\n=== Creating Grant: {grant_name} ===")

    url = f"{BASE_URL}/grants/"
    data = {
        "grant_name": grant_name,
        "grantor": grantor,
        "grant_amount": str(amount),
        "grant_type": "restricted",
        "start_date": start_date,
        "end_date": end_date,
        "fund_id": fund_id,
        "reporting_requirements": "Annual reports due",
    }

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        grant = response.json()
        print(f"Success: Grant created")
        print(f"  Grantor: {grantor}")
        print(f"  Amount: {amount}")
        return grant
    else:
        print(f"Failed: {response.status_code}")
        return {}


# =============================================================================
# SPECIAL JOURNALS EXAMPLES
# =============================================================================


def create_sales_journal_entry(
    customer_id: str, invoice_number: str, invoice_date: str, items: List[Dict], payment_terms: str = "NET 30"
) -> Dict:
    """
    Create a sales journal entry.

    Args:
        customer_id: Customer identifier
        invoice_number: Invoice number
        invoice_date: Invoice date
        items: List of sale items [{account, amount, description}]
        payment_terms: Payment terms

    Returns:
        Created sales journal entry
    """
    print(f"\n=== Creating Sales Journal Entry: {invoice_number} ===")

    url = f"{BASE_URL}/sales-journal/"
    data = {
        "customer_id": customer_id,
        "invoice_number": invoice_number,
        "invoice_date": invoice_date,
        "due_date": (datetime.now() + timedelta(days=30)).isoformat(),
        "items": items,
        "payment_terms": payment_terms,
    }

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        entry = response.json()
        print(f"Success: Sales entry {entry.get('id')} created")
        return entry
    else:
        print(f"Failed: {response.status_code}")
        return {}


def create_petty_cash_entry(
    fund_id: str, amount: Decimal, category: str, description: str, received_from: str = ""
) -> Dict:
    """
    Create a petty cash entry.

    Args:
        fund_id: Petty cash fund ID
        amount: Transaction amount
        category: Expense category
        description: Entry description
        received_from: Source/recipient name

    Returns:
        Created petty cash entry
    """
    print(f"\n=== Creating Petty Cash Entry ===")

    url = f"{BASE_URL}/petty-cash-entries/"
    data = {
        "fund_id": fund_id,
        "entry_date": datetime.now().isoformat(),
        "entry_type": "disbursement" if amount < 0 else "receipt",
        "amount": str(abs(amount)),
        "category": category,
        "description": description,
        "voucher_number": f"PV-{datetime.now().strftime('%Y%m%d%H%M%S')}",
        "received_from": received_from,
        "authorized_by": "Manager",
    }

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        entry = response.json()
        print(f"Success: Petty cash entry created")
        return entry
    else:
        print(f"Failed: {response.status_code}")
        return {}


# =============================================================================
# BANK RECONCILIATION EXAMPLES
# =============================================================================


def create_bank_reconciliation(bank_account: str, statement_date: str, statement_balance: Decimal) -> Dict:
    """
    Create a bank reconciliation statement.

    Args:
        bank_account: Bank account identifier
        statement_date: Bank statement date
        statement_balance: Balance from bank statement

    Returns:
        Created bank reconciliation
    """
    print(f"\n=== Creating Bank Reconciliation: {bank_account} ===")

    url = f"{BASE_URL}/bank-reconciliation/"
    data = {"bank_account": bank_account, "statement_date": statement_date, "statement_balance": str(statement_balance)}

    response = requests.post(url, json=data, headers=HEADERS)

    if response.status_code == 201:
        reconciliation = response.json()
        print(f"Success: Reconciliation created")
        print(f"  Statement Balance: {statement_balance}")
        return reconciliation
    else:
        print(f"Failed: {response.status_code}")
        return {}


# =============================================================================
# ERROR HANDLING EXAMPLES
# =============================================================================


def handle_api_errors(response: requests.Response) -> None:
    """
    Handle API errors with detailed information.

    Args:
        response: requests.Response object
    """
    if response.status_code >= 400:
        try:
            error = response.json()
            print(f"Error Code: {error.get('code', 'UNKNOWN')}")
            print(f"Message: {error.get('detail', 'No details')}")
        except json.JSONDecodeError:
            print(f"Error: {response.text}")


def validate_amount(amount: Decimal) -> bool:
    """
    Validate transaction amount.

    Args:
        amount: Amount to validate

    Returns:
        True if valid, False otherwise
    """
    if amount <= 0:
        print("Error: Amount must be positive")
        return False
    if amount > Decimal("999999999.99"):
        print("Error: Amount exceeds maximum allowed")
        return False
    return True


# =============================================================================
# BATCH OPERATIONS EXAMPLES
# =============================================================================


def batch_create_accounts(accounts: List[Dict]) -> List[Dict]:
    """
    Create multiple accounts in batch.

    Args:
        accounts: List of account dictionaries

    Returns:
        List of created accounts
    """
    print(f"\n=== Batch Creating {len(accounts)} Accounts ===")

    created = []
    failed = []

    for account in accounts:
        result = create_account(account["account_number"], account["account_name"], account["account_type"])
        if result:
            created.append(result)
        else:
            failed.append(account["account_number"])

    print(f"Created: {len(created)}, Failed: {len(failed)}")
    return created


# =============================================================================
# COMPLETE WORKFLOW EXAMPLE
# =============================================================================


def example_complete_workflow():
    """
    Complete example: Create accounts, record transactions, generate reports.

    This demonstrates a typical workflow from setup to reporting.
    """
    print("\n" + "=" * 60)
    print("COMPLETE WORKFLOW EXAMPLE")
    print("=" * 60)

    # Step 1: Authenticate
    token = authenticate()
    if not token:
        print("Authentication failed. Exiting.")
        return

    # Step 2: Create Chart of Accounts
    print("\n--- Setting up Chart of Accounts ---")

    accounts = [
        {"account_number": "ACC-1001", "account_name": "Cash", "account_type": "Asset"},
        {"account_number": "ACC-1002", "account_name": "Accounts Receivable", "account_type": "Asset"},
        {"account_number": "ACC-2001", "account_name": "Accounts Payable", "account_type": "Liability"},
        {"account_number": "ACC-3001", "account_name": "Owner's Equity", "account_type": "Equity"},
        {"account_number": "ACC-4001", "account_name": "Sales Revenue", "account_type": "Revenue"},
        {"account_number": "ACC-5001", "account_name": "Rent Expense", "account_type": "Expense"},
    ]

    for acc in accounts:
        create_account(acc["account_number"], acc["account_name"], acc["account_type"])

    # Step 3: Create NPO Fund
    print("\n--- Creating NPO Fund ---")
    fund = create_npo_fund("General Fund", "FUND-GEN", "general", "General operating fund")

    # Step 4: Record Transactions
    print("\n--- Recording Transactions ---")

    # Record a sale
    create_journal_entry(
        entry_date=datetime.now().date().isoformat(),
        description="Sale to customer ABC",
        reference="INV-001",
        debit_account="ACC-1002",
        credit_account="ACC-4001",
        amount=Decimal("1000.00"),
    )

    # Record rent expense
    create_journal_entry(
        entry_date=datetime.now().date().isoformat(),
        description="Rent payment",
        reference="EXP-001",
        debit_account="ACC-5001",
        credit_account="ACC-1001",
        amount=Decimal("500.00"),
    )

    # Record donation
    if fund:
        create_donation(
            donor_id="DON-001", amount=Decimal("5000.00"), donation_type="one_time", fund_id=fund.get("id", "")
        )

    # Step 5: Generate Reports
    print("\n--- Generating Reports ---")

    get_trial_balance()
    get_income_statement(
        start_date=(datetime.now() - timedelta(days=30)).date().isoformat(), end_date=datetime.now().date().isoformat()
    )
    get_balance_sheet(datetime.now().date().isoformat())

    print("\n" + "=" * 60)
    print("WORKFLOW COMPLETE")
    print("=" * 60)


# =============================================================================
# MAIN
# =============================================================================

if __name__ == "__main__":
    import sys

    print("""
    Vimbai API Usage Examples
    ========================

    This module provides examples for using the Vimbai API.

    Examples:
    1. authenticate() - Login and get JWT token
    2. create_account() - Create chart of accounts
    3. create_journal_entry() - Record transactions
    4. get_trial_balance() - Generate trial balance
    5. create_npo_fund() - Create NPO fund
    6. create_donation() - Record donations
    7. example_complete_workflow() - Full workflow demo

    To run the complete workflow example:
        python api_examples.py --run-workflow
    """)

    if "--run-workflow" in sys.argv:
        example_complete_workflow()
    else:
        print("\nRun with --run-workflow to execute the complete example")
