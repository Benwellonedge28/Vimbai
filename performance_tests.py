"""
Performance Testing Suite for Vimbai

This module provides comprehensive load testing and performance benchmarking
for Vimbai microservices using Locust.

Features:
- API endpoint load testing
- Neo4j query performance testing
- Concurrent user simulation
- Response time analysis
- Throughput measurement

Usage:
    # Run all tests
    locust -f performance_tests.py --host=http://localhost:8000

    # Run with specific parameters
    locust -f performance_tests.py --host=http://localhost:8000 --users=100 --spawn-rate=10 --run-time=60s
"""

import random
import string
from datetime import datetime, timedelta
from decimal import Decimal
from typing import Any, Dict, List

# Import locust for load testing
try:
    import json

    from locust import HttpUser, between, events, task
    from locust.runners import MasterRunner, WorkerRunner
except ImportError:
    print("Install locust for performance testing: pip install locust")
    raise


# =============================================================================
# TEST DATA GENERATORS
# =============================================================================


def generate_account_number() -> str:
    """Generate a random account number."""
    return f"ACC-{random.randint(1000, 9999)}"


def generate_journal_entry_data() -> Dict[str, Any]:
    """Generate random journal entry data."""
    return {
        "entry_date": datetime.now().isoformat(),
        "description": f"Test Journal Entry {random.randint(1000, 9999)}",
        "reference_number": f"REF-{random.randint(100000, 999999)}",
        "entries": [
            {
                "account_number": generate_account_number(),
                "debit_amount": str(random.randint(100, 10000)),
                "credit_amount": "0",
                "description": "Debit entry",
            },
            {
                "account_number": generate_account_number(),
                "debit_amount": "0",
                "credit_amount": str(random.randint(100, 10000)),
                "description": "Credit entry",
            },
        ],
    }


def generate_fund_data() -> Dict[str, Any]:
    """Generate random NPO fund data."""
    fund_types = ["general", "restricted", "endowment", "capital", "project"]
    return {
        "fund_name": f"Test Fund {random.randint(1000, 9999)}",
        "fund_code": f"FUND-{random.randint(100, 999)}",
        "fund_type": random.choice(fund_types),
        "description": f"Test fund for performance testing",
        "current_balance": str(random.randint(1000, 100000)),
        "total_contributions": str(random.randint(1000, 50000)),
        "total_disbursements": str(random.randint(500, 25000)),
    }


def generate_donation_data() -> Dict[str, Any]:
    """Generate random donation data."""
    donation_types = ["one_time", "recurring", "matching", "in_kind"]
    return {
        "donor_id": f"DON-{random.randint(1000, 9999)}",
        "donation_date": datetime.now().isoformat(),
        "amount": str(random.randint(50, 10000)),
        "donation_type": random.choice(donation_types),
        "designation": random.choice(["general", "program", "capital", "endowment"]),
        "payment_method": random.choice(["cash", "check", "credit_card", "bank_transfer"]),
        "receipt_number": f"RCP-{random.randint(100000, 999999)}",
    }


# =============================================================================
# BASE TEST CONFIGURATION
# =============================================================================


class VimbaiUser(HttpUser):
    """
    Base class for Vimbai load testing.

    Attributes:
        abstract: Prevents instantiation of this base class
        wait_time: Time between task executions (1-3 seconds)
    """

    abstract = True
    wait_time = between(1, 3)

    def on_start(self):
        """Called when a simulated user starts."""
        # Setup: Authenticate and get token
        self.jwt_token = None
        self._authenticate()

    def _authenticate(self):
        """Authenticate and store JWT token for requests."""
        # Note: Replace with actual authentication endpoint
        response = self.client.post("/auth/login", json={"username": "test_user", "password": "test_password"})
        if response.status_code == 200:
            self.jwt_token = response.json().get("access_token")

    def _get_headers(self) -> Dict[str, str]:
        """Get request headers with authentication."""
        headers = {"Content-Type": "application/json"}
        if self.jwt_token:
            headers["Authorization"] = f"Bearer {self.jwt_token}"
        return headers


# =============================================================================
# ACCOUNTING SERVICE TESTS
# =============================================================================


class AccountingServiceUser(VimbaiUser):
    """
    Load test user for accounting service endpoints.
    Tests CRUD operations, queries, and reporting endpoints.
    """

    @task(10)
    def get_accounts(self):
        """Test: List all accounts (high frequency)."""
        self.client.get("/accounts/", headers=self._get_headers(), name="/accounts/ [LIST]")

    @task(8)
    def get_account_by_number(self):
        """Test: Get single account by number."""
        account_num = generate_account_number()
        self.client.get(f"/accounts/{account_num}", headers=self._get_headers(), name="/accounts/{id} [GET]")

    @task(5)
    def create_account(self):
        """Test: Create new account."""
        account_data = {
            "account_number": generate_account_number(),
            "account_name": f"Test Account {random.randint(1000, 9999)}",
            "account_type": random.choice(["Asset", "Liability", "Equity", "Revenue", "Expense"]),
            "description": "Load test account",
            "is_active": True,
        }
        self.client.post("/accounts/", json=account_data, headers=self._get_headers(), name="/accounts/ [CREATE]")

    @task(8)
    def get_journal_entries(self):
        """Test: List journal entries."""
        self.client.get("/journal-entries/", headers=self._get_headers(), name="/journal-entries/ [LIST]")

    @task(6)
    def create_journal_entry(self):
        """Test: Create journal entry."""
        entry_data = generate_journal_entry_data()
        self.client.post(
            "/journal-entries/", json=entry_data, headers=self._get_headers(), name="/journal-entries/ [CREATE]"
        )

    @task(7)
    def get_ledger(self):
        """Test: Get account ledger."""
        account_num = generate_account_number()
        self.client.get(f"/ledgers/{account_num}", headers=self._get_headers(), name="/ledgers/{account} [GET]")

    @task(4)
    def get_trial_balance(self):
        """Test: Generate trial balance report."""
        self.client.get("/trial-balance/", headers=self._get_headers(), name="/trial-balance/ [GET]")

    @task(5)
    def get_income_statement(self):
        """Test: Generate income statement."""
        start_date = (datetime.now() - timedelta(days=30)).isoformat()
        end_date = datetime.now().isoformat()
        self.client.get(
            f"/income-statement/?start_date={start_date}&end_date={end_date}",
            headers=self._get_headers(),
            name="/income-statement/ [GET]",
        )

    @task(5)
    def get_balance_sheet(self):
        """Test: Generate balance sheet."""
        as_of_date = datetime.now().isoformat()
        self.client.get(
            f"/balance-sheet/?as_of_date={as_of_date}", headers=self._get_headers(), name="/balance-sheet/ [GET]"
        )

    @task(3)
    def get_sales_journal(self):
        """Test: Get sales journal."""
        self.client.get("/sales-journal/", headers=self._get_headers(), name="/sales-journal/ [LIST]")

    @task(3)
    def get_petty_cash_funds(self):
        """Test: Get petty cash funds."""
        self.client.get("/petty-cash-funds/", headers=self._get_headers(), name="/petty-cash-funds/ [LIST]")

    @task(2)
    def get_bank_reconciliations(self):
        """Test: Get bank reconciliations."""
        self.client.get("/bank-reconciliation/", headers=self._get_headers(), name="/bank-reconciliation/ [LIST]")


# =============================================================================
# NPO SERVICE TESTS
# =============================================================================


class NPOServiceUser(VimbaiUser):
    """
    Load test user for NPO service endpoints.
    Tests fund accounting, donations, grants, and reporting.
    """

    @task(8)
    def get_funds(self):
        """Test: List all funds (high frequency)."""
        self.client.get("/funds/", headers=self._get_headers(), name="/funds/ [LIST]")

    @task(6)
    def create_fund(self):
        """Test: Create new fund."""
        fund_data = generate_fund_data()
        self.client.post("/funds/", json=fund_data, headers=self._get_headers(), name="/funds/ [CREATE]")

    @task(5)
    def get_donations(self):
        """Test: List donations."""
        self.client.get("/donations/", headers=self._get_headers(), name="/donations/ [LIST]")

    @task(4)
    def create_donation(self):
        """Test: Create donation."""
        donation_data = generate_donation_data()
        self.client.post("/donations/", json=donation_data, headers=self._get_headers(), name="/donations/ [CREATE]")

    @task(5)
    def get_grants(self):
        """Test: List grants."""
        self.client.get("/grants/", headers=self._get_headers(), name="/grants/ [LIST]")

    @task(4)
    def get_donors(self):
        """Test: List donors."""
        self.client.get("/donors/", headers=self._get_headers(), name="/donors/ [LIST]")

    @task(3)
    def get_budgets(self):
        """Test: List budgets."""
        self.client.get("/budgets/", headers=self._get_headers(), name="/budgets/ [LIST]")

    @task(4)
    def get_statement_of_activities(self):
        """Test: Get statement of activities."""
        period_start = (datetime.now() - timedelta(days=30)).isoformat()
        period_end = datetime.now().isoformat()
        self.client.get(
            f"/statements/activities/?period_start={period_start}&period_end={period_end}",
            headers=self._get_headers(),
            name="/statements/activities/ [GET]",
        )

    @task(4)
    def get_net_assets(self):
        """Test: Get net assets."""
        as_of_date = datetime.now().date().isoformat()
        self.client.get(f"/net-assets/{as_of_date}", headers=self._get_headers(), name="/net-assets/{date} [GET]")

    @task(2)
    def get_projects(self):
        """Test: List projects."""
        self.client.get("/projects/", headers=self._get_headers(), name="/projects/ [LIST]")

    @task(2)
    def get_programs(self):
        """Test: List programs."""
        self.client.get("/programs/", headers=self._get_headers(), name="/programs/ [LIST]")

    @task(2)
    def get_internal_controls(self):
        """Test: List internal controls."""
        self.client.get("/internal-controls/", headers=self._get_headers(), name="/internal-controls/ [LIST]")


# =============================================================================
# MIXED WORKLOAD TEST
# =============================================================================


class MixedWorkloadUser(VimbaiUser):
    """
    Simulates realistic mixed workload across all services.
    Tests typical user behavior with varied operations.
    """

    @task(15)
    def read_operations(self):
        """Test: Primarily read operations (70% of traffic)."""
        operations = [
            lambda: self.client.get("/accounts/", headers=self._get_headers(), name="[READ] accounts"),
            lambda: self.client.get("/journal-entries/", headers=self._get_headers(), name="[READ] journal"),
            lambda: self.client.get("/funds/", headers=self._get_headers(), name="[READ] funds"),
            lambda: self.client.get("/ledgers/ACC-5000", headers=self._get_headers(), name="[READ] ledger"),
            lambda: self.client.get("/trial-balance/", headers=self._get_headers(), name="[READ] trial"),
            lambda: self.client.get("/donors/", headers=self._get_headers(), name="[READ] donors"),
            lambda: self.client.get("/budgets/", headers=self._get_headers(), name="[READ] budgets"),
        ]
        random.choice(operations)()

    @task(4)
    def write_operations(self):
        """Test: Write operations (20% of traffic)."""
        operations = [
            lambda: self.client.post(
                "/accounts/",
                json={
                    "account_number": generate_account_number(),
                    "account_name": f"Test {random.randint(1000, 9999)}",
                    "account_type": "Asset",
                },
                headers=self._get_headers(),
                name="[WRITE] account",
            ),
            lambda: self.client.post(
                "/journal-entries/",
                json=generate_journal_entry_data(),
                headers=self._get_headers(),
                name="[WRITE] journal",
            ),
            lambda: self.client.post(
                "/funds/", json=generate_fund_data(), headers=self._get_headers(), name="[WRITE] fund"
            ),
            lambda: self.client.post(
                "/donations/", json=generate_donation_data(), headers=self._get_headers(), name="[WRITE] donation"
            ),
        ]
        random.choice(operations)()

    @task(2)
    def report_generation(self):
        """Test: Report generation (10% of traffic)."""
        operations = [
            lambda: self._get_income_statement(),
            lambda: self._get_balance_sheet(),
            lambda: self._get_trial_balance(),
            lambda: self._get_statement_of_activities(),
        ]
        random.choice(operations)()

    def _get_income_statement(self):
        start = (datetime.now() - timedelta(days=30)).isoformat()
        end = datetime.now().isoformat()
        self.client.get(
            f"/income-statement/?start_date={start}&end_date={end}", headers=self._get_headers(), name="[REPORT] income"
        )

    def _get_balance_sheet(self):
        date = datetime.now().isoformat()
        self.client.get(f"/balance-sheet/?as_of_date={date}", headers=self._get_headers(), name="[REPORT] balance")

    def _get_trial_balance(self):
        self.client.get("/trial-balance/", headers=self._get_headers(), name="[REPORT] trial")

    def _get_statement_of_activities(self):
        start = (datetime.now() - timedelta(days=30)).date().isoformat()
        end = datetime.now().date().isoformat()
        self.client.get(
            f"/statements/activities/?period_start={start}&period_end={end}",
            headers=self._get_headers(),
            name="[REPORT] activities",
        )


# =============================================================================
# SPIKE TEST SCENARIOS
# =============================================================================


class SpikeLoadUser(VimbaiUser):
    """
    Simulates spike load scenarios - sudden increases in traffic.
    Used for capacity planning and identifying bottlenecks.
    """

    wait_time = between(0.1, 0.5)  # Fast consecutive requests

    @task
    def rapid_reads(self):
        """Test: Rapid read operations during spike."""
        self.client.get("/accounts/", headers=self._get_headers(), name="[SPIKE] rapid read")


# =============================================================================
# CUSTOM EVENT HANDLERS
# =============================================================================


@events.init_command_line_parser.add_listener
def add_custom_arguments(parser):
    """Add custom command line arguments for test configuration."""
    parser.add_argument("--test-duration", type=int, default=60, help="Duration of test in seconds")
    parser.add_argument("--peak-users", type=int, default=50, help="Number of peak concurrent users")


@events.test_start.add_listener
def on_test_start(environment, **kwargs):
    """Called when test starts."""
    print(f"Starting load test at {datetime.now()}")
    print(f"Target host: {environment.host}")


@events.test_stop.add_listener
def on_test_stop(environment, **kwargs):
    """Called when test stops - generate performance report."""
    print(f"\nTest completed at {datetime.now()}")

    # Extract statistics
    stats = environment.stats

    print("\n" + "=" * 60)
    print("PERFORMANCE TEST RESULTS")
    print("=" * 60)
    print(f"Total Requests: {stats.total.num_requests}")
    print(f"Failed Requests: {stats.total.num_failures}")
    print(f"Average Response Time: {stats.total.avg_response_time:.2f}ms")
    print(f"Median Response Time: {stats.total.median_response_time:.2f}ms")
    print(f"95th Percentile: {stats.total.get_response_time_percentile(0.95):.2f}ms")
    print(f"99th Percentile: {stats.total.get_response_time_percentile(0.99):.2f}ms")
    print(f"Requests/sec: {stats.total.total_rps:.2f}")
    print("=" * 60)


@events.request.add_listener
def on_request(request_type, name, response_time, response_length, exception, **kwargs):
    """Track individual request metrics."""
    if response_time > 1000:  # Log slow requests (>1 second)
        print(f"SLOW REQUEST: {name} took {response_time:.2f}ms")


# =============================================================================
# NEO4J QUERY PERFORMANCE TESTS
# =============================================================================


class Neo4jQueryTester(HttpUser):
    """
    Dedicated test class for Neo4j query performance.
    Tests specific query patterns and measures execution time.
    """

    abstract = True

    @task
    def test_account_lookup_by_number(self):
        """Test Neo4j query: Account lookup by account_number."""
        start = datetime.now()
        self.client.get("/accounts/", name="[NEO4J] account_by_number")
        duration = (datetime.now() - start).total_seconds() * 1000
        print(f"Query duration: {duration:.2f}ms")

    @task
    def test_journal_entry_date_range(self):
        """Test Neo4j query: Journal entries in date range."""
        start = datetime.now()
        self.client.get("/journal-entries/", name="[NEO4J] journal_date_range")
        duration = (datetime.now() - start).total_seconds() * 1000
        print(f"Query duration: {duration:.2f}ms")

    @task
    def test_ledger_entries(self):
        """Test Neo4j query: Get all ledger entries for account."""
        start = datetime.now()
        self.client.get("/ledgers/ACC-5000", name="[NEO4J] ledger_entries")
        duration = (datetime.now() - start).total_seconds() * 1000
        print(f"Query duration: {duration:.2f}ms")

    @task
    def test_trial_balance_query(self):
        """Test Neo4j query: Trial balance aggregation."""
        start = datetime.now()
        self.client.get("/trial-balance/", name="[NEO4J] trial_balance")
        duration = (datetime.now() - start).total_seconds() * 1000
        print(f"Query duration: {duration:.2f}ms")


# =============================================================================
# HELPER FUNCTIONS
# =============================================================================


def generate_performance_report(stats) -> Dict[str, Any]:
    """
    Generate a performance report from test statistics.

    Args:
        stats: Locust statistics object

    Returns:
        Dictionary with performance metrics
    """
    return {
        "total_requests": stats.total.num_requests,
        "failed_requests": stats.total.num_failures,
        "failure_rate": stats.total.fail_ratio,
        "average_response_time_ms": stats.total.avg_response_time,
        "median_response_time_ms": stats.total.median_response_time,
        "p95_response_time_ms": stats.total.get_response_time_percentile(0.95),
        "p99_response_time_ms": stats.total.get_response_time_percentile(0.99),
        "max_response_time_ms": stats.total.max_response_time,
        "requests_per_second": stats.total.total_rps,
        "bandwidth_mb_per_sec": stats.total.total_rps * (stats.total.avg_content_length / 1024 / 1024),
    }


def export_results_to_json(stats, filename: str = "performance_results.json"):
    """
    Export test results to JSON file.

    Args:
        stats: Locust statistics object
        filename: Output filename
    """
    report = generate_performance_report(stats)

    with open(filename, "w") as f:
        json.dump(report, f, indent=2)

    print(f"Results exported to {filename}")


# =============================================================================
# MAIN ENTRY POINT
# =============================================================================

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Vimbai Performance Testing")
    parser.add_argument("--host", default="http://localhost:8000", help="Target host URL")
    parser.add_argument("--users", type=int, default=10, help="Number of concurrent users")
    parser.add_argument("--spawn-rate", type=int, default=5, help="User spawn rate per second")
    parser.add_argument("--run-time", default="60s", help="Test run duration")

    args = parser.parse_args()

    print(f"""
    ========================================
    Vimbai Performance Test Suite
    ========================================

    Target: {args.host}
    Users: {args.users}
    Spawn Rate: {args.spawn_rate}/sec
    Duration: {args.run_time}

    Run with:
    locust -f {__file__} --host={args.host}
    ========================================
    """)
