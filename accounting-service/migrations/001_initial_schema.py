"""
Migration 001: Initial Neo4j Schema
Creates constraints, indexes, and initial schema for Vimbai
"""

import logging
from datetime import datetime

logger = logging.getLogger(__name__)

MIGRATION_ID = "001"
MIGRATION_NAME = "initial_schema"
MIGRATION_DESCRIPTION = "Create initial Neo4j constraints and indexes"


async def up(session):
    """Apply the migration."""
    constraints = [
        "CREATE CONSTRAINT user_id_unique IF NOT EXISTS FOR (u:User) REQUIRE u.id IS UNIQUE",
        "CREATE CONSTRAINT account_number_unique IF NOT EXISTS FOR (a:Account) REQUIRE a.account_number IS UNIQUE",
        "CREATE CONSTRAINT journal_entry_id_unique IF NOT EXISTS FOR (j:JournalEntry) REQUIRE j.id IS UNIQUE",
        "CREATE CONSTRAINT journal_line_id_unique IF NOT EXISTS FOR (l:JournalLine) REQUIRE l.id IS UNIQUE",
        "CREATE CONSTRAINT vendor_id_unique IF NOT EXISTS FOR (v:Vendor) REQUIRE v.id IS UNIQUE",
        "CREATE CONSTRAINT customer_id_unique IF NOT EXISTS FOR (c:Customer) REQUIRE c.id IS UNIQUE",
        "CREATE CONSTRAINT budget_id_unique IF NOT EXISTS FOR (b:Budget) REQUIRE b.id IS UNIQUE",
        "CREATE CONSTRAINT audit_event_id_unique IF NOT EXISTS FOR (e:AuditEvent) REQUIRE e.id IS UNIQUE",
    ]

    indexes = [
        "CREATE INDEX account_type_index IF NOT EXISTS FOR (a:Account) ON (a.account_type)",
        "CREATE INDEX account_created_index IF NOT EXISTS FOR (a:Account) ON (a.created_at)",
        "CREATE INDEX journal_entry_date_index IF NOT EXISTS FOR (j:JournalEntry) ON (j.entry_date)",
        "CREATE INDEX journal_entry_status_index IF NOT EXISTS FOR (j:JournalEntry) ON (j.status)",
        "CREATE INDEX vendor_name_index IF NOT EXISTS FOR (v:Vendor) ON (v.name)",
        "CREATE INDEX customer_name_index IF NOT EXISTS FOR (c:Customer) ON (c.name)",
        "CREATE INDEX budget_period_index IF NOT EXISTS FOR (b:Budget) ON (b.period)",
        "CREATE INDEX audit_event_date_index IF NOT EXISTS FOR (e:AuditEvent) ON (e.timestamp)",
    ]

    for query in constraints:
        await session.run(query)
        logger.info(f"Executed: {query[:60]}...")

    for query in indexes:
        await session.run(query)
        logger.info(f"Executed: {query[:60]}...")

    # Record migration
    await session.run(
        "CREATE (m:Migration {id: $id, name: $name, description: $description, applied_at: datetime()})",
        id=MIGRATION_ID,
        name=MIGRATION_NAME,
        description=MIGRATION_DESCRIPTION,
    )
    logger.info(f"Migration {MIGRATION_ID} applied successfully")


async def down(session):
    """Rollback the migration."""
    rollback = [
        "DROP INDEX account_type_index IF EXISTS",
        "DROP INDEX account_created_index IF EXISTS",
        "DROP INDEX journal_entry_date_index IF EXISTS",
        "DROP INDEX journal_entry_status_index IF EXISTS",
        "DROP INDEX vendor_name_index IF EXISTS",
        "DROP INDEX customer_name_index IF EXISTS",
        "DROP INDEX budget_period_index IF EXISTS",
        "DROP INDEX audit_event_date_index IF EXISTS",
    ]
    for query in rollback:
        await session.run(query)
    await session.run("MATCH (m:Migration {id: $id}) DELETE m", id=MIGRATION_ID)
    logger.info(f"Migration {MIGRATION_ID} rolled back")
