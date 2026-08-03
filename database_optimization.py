"""
Database Performance Optimization Module for Vimbai

This module provides comprehensive Neo4j index management and query optimization
to ensure high-performance database operations across all Vimbai services.

Features:
- Automatic index creation on service startup
- Query performance monitoring
- Index health checks
- Query statistics tracking

Usage:
    from database_optimization import IndexManager, QueryOptimizer

    # Initialize indexes on startup
    await IndexManager.create_all_indexes(driver)

    # Monitor query performance
    optimizer = QueryOptimizer(driver)
    await optimizer.analyze_slow_queries()
"""

from typing import Dict, List, Optional, Any
from dataclasses import dataclass
from datetime import datetime
import asyncio


# =============================================================================
# DATA CLASSES
# =============================================================================

@dataclass
class IndexDefinition:
    """Defines a Neo4j index with metadata."""
    label: str
    property: str
    index_type: str = "RANGE"  # RANGE, TEXT, POINT, FULLTEXT
    description: str = ""


@dataclass
class QueryStatistics:
    """Tracks query execution statistics."""
    query: str
    execution_time_ms: float
    hits: int
    timestamp: datetime
    cached: bool = False


@dataclass
class IndexHealthReport:
    """Report on index health and recommendations."""
    indexes: List[Dict[str, Any]]
    recommendations: List[str]
    last_analyzed: datetime


# =============================================================================
# CORE INDEX DEFINITIONS
# =============================================================================

# Account-related indexes for fast lookups
ACCOUNT_INDEXES = [
    IndexDefinition(
        label="Account",
        property="account_number",
        description="Primary lookup by account number"
    ),
    IndexDefinition(
        label="Account",
        property="account_name",
        description="Search by account name"
    ),
    IndexDefinition(
        label="Account",
        property="account_type",
        description="Filter by account type (Asset, Liability, etc.)"
    ),
    IndexDefinition(
        label="Account",
        property="user_id",
        description="User-scoped account queries"
    ),
    IndexDefinition(
        label="Account",
        property="created_at",
        description="Temporal queries and reporting"
    ),
]

# Journal Entry indexes for transaction lookups
JOURNAL_ENTRY_INDEXES = [
    IndexDefinition(
        label="JournalEntry",
        property="entry_id",
        description="Primary lookup by entry ID"
    ),
    IndexDefinition(
        label="JournalEntry",
        property="user_id",
        description="User-scoped entry queries"
    ),
    IndexDefinition(
        label="JournalEntry",
        property="entry_date",
        description="Date-range queries for financial reports"
    ),
    IndexDefinition(
        label="JournalEntry",
        property="reference_number",
        description="Search by reference/document number"
    ),
    IndexDefinition(
        label="JournalEntry",
        property="status",
        description="Filter by entry status (draft, posted, etc.)"
    ),
]

# User and Organization indexes
USER_ORG_INDEXES = [
    IndexDefinition(
        label="User",
        property="user_id",
        description="Primary user lookup"
    ),
    IndexDefinition(
        label="User",
        property="email",
        description="Email-based authentication lookup"
    ),
    IndexDefinition(
        label="User",
        property="organization_id",
        description="Organization membership queries"
    ),
    IndexDefinition(
        label="Organization",
        property="org_id",
        description="Organization lookup"
    ),
]

# Transaction indexes for banking integration
TRANSACTION_INDEXES = [
    IndexDefinition(
        label="Transaction",
        property="transaction_id",
        description="Primary transaction lookup"
    ),
    IndexDefinition(
        label="Transaction",
        property="user_id",
        description="User transaction history"
    ),
    IndexDefinition(
        label="Transaction",
        property="transaction_date",
        description="Date-range transaction queries"
    ),
    IndexDefinition(
        label="Transaction",
        property="bank_account_id",
        description="Bank account transaction filtering"
    ),
    IndexDefinition(
        label="Transaction",
        property="amount",
        description="Amount-based filtering and reporting"
    ),
    IndexDefinition(
        label="Transaction",
        property="status",
        description="Transaction status filtering"
    ),
    IndexDefinition(
        label="Transaction",
        property="external_reference",
        description="External system reference lookup"
    ),
]

# NPO Service indexes
NPO_INDEXES = [
    IndexDefinition(
        label="NPOFund",
        property="fund_id",
        description="Primary fund lookup"
    ),
    IndexDefinition(
        label="NPOFund",
        property="fund_code",
        description="Fund code search"
    ),
    IndexDefinition(
        label="NPOFund",
        property="fund_type",
        description="Fund type filtering"
    ),
    IndexDefinition(
        label="NPOFund",
        property="user_id",
        description="User-scoped fund queries"
    ),
    IndexDefinition(
        label="Donor",
        property="donor_id",
        description="Primary donor lookup"
    ),
    IndexDefinition(
        label="Donor",
        property="email",
        description="Donor email search"
    ),
    IndexDefinition(
        label="Grant",
        property="grant_id",
        description="Primary grant lookup"
    ),
    IndexDefinition(
        label="Grant",
        property="status",
        description="Grant status filtering"
    ),
    IndexDefinition(
        label="Grant",
        property="application_date",
        description="Grant application date queries"
    ),
]

# Ledger and Reporting indexes
LEDGER_REPORTING_INDEXES = [
    IndexDefinition(
        label="LedgerEntry",
        property="entry_id",
        description="Primary ledger entry lookup"
    ),
    IndexDefinition(
        label="LedgerEntry",
        property="account_number",
        description="Account ledger queries"
    ),
    IndexDefinition(
        label="LedgerEntry",
        property="posting_date",
        description="Date-based ledger queries"
    ),
    IndexDefinition(
        label="LedgerEntry",
        property="user_id",
        description="User ledger access"
    ),
    IndexDefinition(
        label="TrialBalance",
        property="period_start",
        description="Trial balance period queries"
    ),
    IndexDefinition(
        label="TrialBalance",
        property="user_id",
        description="User trial balance access"
    ),
]

# Workflow and Automation indexes
WORKFLOW_INDEXES = [
    IndexDefinition(
        label="WorkflowDefinition",
        property="workflow_id",
        description="Primary workflow lookup"
    ),
    IndexDefinition(
        label="WorkflowDefinition",
        property="user_id",
        description="User workflow access"
    ),
    IndexDefinition(
        label="WorkflowInstance",
        property="instance_id",
        description="Primary workflow instance lookup"
    ),
    IndexDefinition(
        label="WorkflowInstance",
        property="status",
        description="Instance status filtering"
    ),
    IndexDefinition(
        label="WorkflowInstance",
        property="created_at",
        description="Temporal instance queries"
    ),
]

# Supplier and Customer indexes
SUPPLIER_CUSTOMER_INDEXES = [
    IndexDefinition(
        label="Supplier",
        property="supplier_id",
        description="Primary supplier lookup"
    ),
    IndexDefinition(
        label="Supplier",
        property="name",
        description="Supplier name search"
    ),
    IndexDefinition(
        label="Customer",
        property="customer_id",
        description="Primary customer lookup"
    ),
    IndexDefinition(
        label="Customer",
        property="email",
        description="Customer email search"
    ),
    IndexDefinition(
        label="Customer",
        property="name",
        description="Customer name search"
    ),
]

# All indexes combined
ALL_INDEXES = (
    ACCOUNT_INDEXES +
    JOURNAL_ENTRY_INDEXES +
    USER_ORG_INDEXES +
    TRANSACTION_INDEXES +
    NPO_INDEXES +
    LEDGER_REPORTING_INDEXES +
    WORKFLOW_INDEXES +
    SUPPLIER_CUSTOMER_INDEXES
)


# =============================================================================
# INDEX MANAGEMENT
# =============================================================================

class IndexManager:
    """
    Manages Neo4j database indexes for optimal query performance.

    Features:
    - Create indexes on startup
    - Drop and recreate stale indexes
    - Verify index health
    - Get index statistics
    """

    @staticmethod
    async def create_index(driver, index_def: IndexDefinition) -> bool:
        """
        Create a single index in Neo4j.

        Args:
            driver: Neo4j driver instance
            index_def: Index definition with label and property

        Returns:
            True if index created successfully
        """
        try:
            async with driver.session() as session:
                # Use IF NOT EXISTS to prevent errors on re-run
                query = f"""
                CREATE INDEX IF NOT EXISTS {index_def.label}_{index_def.property}_index
                FOR (n:{index_def.label})
                ON (n.{index_def.property})
                """
                await session.run(query)
                print(f"Created index: {index_def.label}.{index_def.property}")
                return True
        except Exception as e:
            print(f"Failed to create index {index_def.label}.{index_def.property}: {e}")
            return False

    @staticmethod
    async def create_all_indexes(driver) -> Dict[str, int]:
        """
        Create all defined indexes in the database.

        Args:
            driver: Neo4j driver instance

        Returns:
            Dictionary with success/failure counts
        """
        results = {"success": 0, "failed": 0}

        for index_def in ALL_INDEXES:
            success = await IndexManager.create_index(driver, index_def)
            if success:
                results["success"] += 1
            else:
                results["failed"] += 1

        print(f"Index creation complete: {results['success']} succeeded, {results['failed']} failed")
        return results

    @staticmethod
    async def drop_index(driver, label: str, property: str) -> bool:
        """
        Drop a specific index.

        Args:
            driver: Neo4j driver instance
            label: Node label
            property: Property name

        Returns:
            True if index dropped successfully
        """
        try:
            async with driver.session() as session:
                query = f"DROP INDEX {label}_{property}_index IF EXISTS"
                await session.run(query)
                print(f"Dropped index: {label}.{property}")
                return True
        except Exception as e:
            print(f"Failed to drop index {label}.{property}: {e}")
            return False

    @staticmethod
    async def list_indexes(driver) -> List[Dict[str, Any]]:
        """
        List all indexes in the database.

        Args:
            driver: Neo4j driver instance

        Returns:
            List of index information dictionaries
        """
        try:
            async with driver.session() as session:
                result = await session.run("SHOW INDEXES YIELD *")
                records = await result.data()
                return records
        except Exception as e:
            print(f"Failed to list indexes: {e}")
            return []

    @staticmethod
    async def verify_indexes(driver) -> IndexHealthReport:
        """
        Verify index health and provide recommendations.

        Args:
            driver: Neo4j driver instance

        Returns:
            IndexHealthReport with findings and recommendations
        """
        existing_indexes = await IndexManager.list_indexes(driver)
        existing_props = set()

        for idx in existing_indexes:
            # Extract label and property from index name
            name = idx.get("name", "")
            existing_props.add(name)

        defined_props = {
            f"{idx.label}_{idx.property}_index"
            for idx in ALL_INDEXES
        }

        missing = defined_props - existing_props
        recommendations = []

        if missing:
            recommendations.append(
                f"Missing {len(missing)} indexes. Run create_all_indexes() to add them."
            )

        recommendations.append("Consider using composite indexes for frequently queried property combinations")
        recommendations.append("Monitor query execution plans for full scans and optimize accordingly")

        return IndexHealthReport(
            indexes=existing_indexes,
            recommendations=recommendations,
            last_analyzed=datetime.now(timezone.utc)
        )


# =============================================================================
# QUERY OPTIMIZATION
# =============================================================================

class QueryOptimizer:
    """
    Monitors and optimizes query performance.

    Features:
    - Track slow queries
    - Analyze query patterns
    - Provide optimization suggestions
    """

    def __init__(self, driver):
        self.driver = driver
        self.query_stats: List[QueryStatistics] = []
        self.slow_query_threshold_ms = 100  # 100ms threshold

    async def execute_with_stats(self, query: str, params: Dict = None) -> Any:
        """
        Execute a query and track its statistics.

        Args:
            query: Cypher query string
            params: Query parameters

        Returns:
            Query result
        """
        start_time = asyncio.get_event_loop().time()

        try:
            async with self.driver.session() as session:
                result = await session.run(query, params or {})
                records = await result.data()

            execution_time = (asyncio.get_event_loop().time() - start_time) * 1000

            stats = QueryStatistics(
                query=query[:200],  # Truncate for storage
                execution_time_ms=execution_time,
                hits=len(records),
                timestamp=datetime.now(timezone.utc)
            )
            self.query_stats.append(stats)

            # Log slow queries
            if execution_time > self.slow_query_threshold_ms:
                print(f"SLOW QUERY ({execution_time:.2f}ms): {query[:100]}...")

            return records

        except Exception as e:
            execution_time = (asyncio.get_event_loop().time() - start_time) * 1000
            print(f"QUERY ERROR ({execution_time:.2f}ms): {query[:100]} - {e}")
            raise

    async def get_slow_queries(self, limit: int = 10) -> List[QueryStatistics]:
        """
        Get the slowest queries executed.

        Args:
            limit: Maximum number of queries to return

        Returns:
            List of slow QueryStatistics
        """
        sorted_stats = sorted(
            self.query_stats,
            key=lambda x: x.execution_time_ms,
            reverse=True
        )
        return sorted_stats[:limit]

    async def get_average_execution_time(self) -> float:
        """
        Calculate average query execution time.

        Returns:
            Average execution time in milliseconds
        """
        if not self.query_stats:
            return 0.0

        total = sum(s.execution_time_ms for s in self.query_stats)
        return total / len(self.query_stats)

    async def analyze_queries(self) -> Dict[str, Any]:
        """
        Analyze query patterns and provide recommendations.

        Returns:
            Dictionary with analysis results
        """
        if not self.query_stats:
            return {"message": "No queries analyzed yet"}

        avg_time = await self.get_average_execution_time()
        slow_queries = await self.get_slow_queries(5)

        return {
            "total_queries": len(self.query_stats),
            "average_execution_time_ms": avg_time,
            "slow_queries_count": len([q for q in self.query_stats if q.execution_time_ms > self.slow_query_threshold_ms]),
            "top_5_slowest_queries": [
                {"query": q.query, "time_ms": q.execution_time_ms}
                for q in slow_queries
            ],
            "recommendations": [
                "Consider adding indexes for slow query patterns",
                "Use parameterization to enable query caching",
                "Review relationship patterns for optimization"
            ]
        }


# =============================================================================
# QUERY TEMPLATES (Pre-optimized Cypher queries)
# =============================================================================

class QueryTemplates:
    """
    Collection of pre-optimized Cypher query templates for common operations.
    Use these templates instead of writing raw queries for better performance.
    """

    # Account queries
    @staticmethod
    def get_account_by_number(account_number: str, user_id: str) -> str:
        """Optimized: Uses index on account_number and user_id"""
        return """
        MATCH (a:Account {account_number: $account_number})
        WHERE EXISTS((:User {user_id: $user_id})-[:OWNS]->(a))
        RETURN a
        """

    @staticmethod
    def get_accounts_by_type(account_type: str, user_id: str) -> str:
        """Optimized: Uses index on account_type and user_id"""
        return """
        MATCH (a:Account {account_type: $account_type})
        WHERE EXISTS((:User {user_id: $user_id})-[:OWNS]->(a))
        RETURN a ORDER BY a.account_number
        """

    # Journal Entry queries
    @staticmethod
    def get_journal_entries_date_range(
        user_id: str,
        start_date: str,
        end_date: str
    ) -> str:
        """Optimized: Uses index on entry_date and user_id"""
        return """
        MATCH (je:JournalEntry)
        WHERE je.user_id = $user_id
          AND je.entry_date >= $start_date
          AND je.entry_date <= $end_date
        RETURN je
        ORDER BY je.entry_date DESC
        """

    @staticmethod
    def get_account_ledger_entries(
        account_number: str,
        user_id: str,
        start_date: str = None,
        end_date: str = None
    ) -> str:
        """Optimized: Uses index on account_number and posting_date"""
        base_query = """
        MATCH (je:JournalEntry)-[:POSTED_TO]->(a:Account {account_number: $account_number})
        WHERE a.user_id = $user_id
        """

        if start_date:
            base_query += "\n  AND je.entry_date >= $start_date"
        if end_date:
            base_query += "\n  AND je.entry_date <= $end_date"

        return base_query + "\nRETURN je ORDER BY je.entry_date"

    # Transaction queries (Banking)
    @staticmethod
    def get_bank_transactions(
        bank_account_id: str,
        start_date: str = None,
        end_date: str = None
    ) -> str:
        """Optimized: Uses index on bank_account_id and transaction_date"""
        base_query = """
        MATCH (t:Transaction {bank_account_id: $bank_account_id})
        """

        if start_date:
            base_query += "\nWHERE t.transaction_date >= $start_date"
        if end_date:
            if start_date:
                base_query += "\n  AND t.transaction_date <= $end_date"
            else:
                base_query += "\nWHERE t.transaction_date <= $end_date"

        return base_query + "\nRETURN t ORDER BY t.transaction_date DESC"

    # NPO Fund queries
    @staticmethod
    def get_fund_transactions(fund_id: str, user_id: str) -> str:
        """Optimized: Uses index on fund_id"""
        return """
        MATCH (t:Transaction)-[:ALLOCATED_TO_FUND]->(f:NPOFund {fund_id: $fund_id})
        WHERE f.user_id = $user_id
        RETURN t ORDER BY t.transaction_date DESC
        """

    @staticmethod
    def get_fund_balance(fund_id: str) -> str:
        """Optimized: Calculates fund balance efficiently"""
        return """
        MATCH (f:NPOFund {fund_id: $fund_id})
        OPTIONAL MATCH (d:Donation)-[:DONATION_TO_FUND]->(f)
        OPTIONAL MATCH (g:Grant)-[:GRANT_IN_FUND]->(f)
        OPTIONAL MATCH (e:Expense)-[:EXPENSE_FROM_FUND]->(f)
        RETURN f.fund_id,
               COALESCE(SUM(d.amount), 0) as total_donations,
               COALESCE(SUM(g.amount), 0) as total_grants,
               COALESCE(SUM(e.amount), 0) as total_expenses,
               COALESCE(SUM(d.amount), 0) + COALESCE(SUM(g.amount), 0) - COALESCE(SUM(e.amount), 0) as current_balance
        """


# =============================================================================
# UTILITY FUNCTIONS
# =============================================================================

async def initialize_database_indexes(driver):
    """
    Initialize all database indexes. Call this on service startup.

    Args:
        driver: Neo4j driver instance
    """
    print("Initializing database indexes...")
    results = await IndexManager.create_all_indexes(driver)

    if results["failed"] > 0:
        print(f"Warning: {results['failed']} indexes failed to create")
    else:
        print("All indexes created successfully")


async def health_check_indexes(driver) -> IndexHealthReport:
    """
    Perform health check on all indexes.

    Args:
        driver: Neo4j driver instance

    Returns:
        IndexHealthReport with recommendations
    """
    return await IndexManager.verify_indexes(driver)


# =============================================================================
# EXPORTS
# =============================================================================

__all__ = [
    "IndexManager",
    "QueryOptimizer",
    "QueryTemplates",
    "IndexDefinition",
    "QueryStatistics",
    "IndexHealthReport",
    "initialize_database_indexes",
    "health_check_indexes",
    # Index definitions for reference
    "ACCOUNT_INDEXES",
    "JOURNAL_ENTRY_INDEXES",
    "USER_ORG_INDEXES",
    "TRANSACTION_INDEXES",
    "NPO_INDEXES",
    "LEDGER_REPORTING_INDEXES",
    "WORKFLOW_INDEXES",
    "SUPPLIER_CUSTOMER_INDEXES",
]