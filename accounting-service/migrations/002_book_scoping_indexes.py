"""
Migration 002: Book Scoping Indexes
Adds book_id range indexes for every record label that the Book-context
data isolation scopes by (X-Book-ID middleware -> crud._run).
"""

import logging

logger = logging.getLogger(__name__)

MIGRATION_ID = "002"
MIGRATION_NAME = "book_scoping_indexes"
MIGRATION_DESCRIPTION = "Create book_id indexes for Book-scoped record labels"


async def up(session):
    """Apply the migration."""
    indexes = [
        "CREATE INDEX account_book_id IF NOT EXISTS FOR (a:Account) ON (a.book_id)",
        "CREATE INDEX journal_entry_book_id IF NOT EXISTS FOR (je:JournalEntry) ON (je.book_id)",
        "CREATE INDEX statement_of_affairs_book_id IF NOT EXISTS FOR (soa:StatementOfAffairs) ON (soa.book_id)",
        "CREATE INDEX capital_calculation_book_id IF NOT EXISTS FOR (cc:CapitalCalculation) ON (cc.book_id)",
        "CREATE INDEX control_account_book_id IF NOT EXISTS FOR (ca:ControlAccount) ON (ca.book_id)",
        "CREATE INDEX receipts_payments_book_id IF NOT EXISTS FOR (rpa:ReceiptsPaymentsAccount) ON (rpa.book_id)",
        "CREATE INDEX petty_cash_fund_book_id IF NOT EXISTS FOR (pcf:PettyCashFund) ON (pcf.book_id)",
        "CREATE INDEX bank_reconciliation_book_id IF NOT EXISTS FOR (br:BankReconciliation) ON (br.book_id)",
        "CREATE INDEX audit_event_book_id IF NOT EXISTS FOR (ae:AuditEvent) ON (ae.book_id)",
    ]
    for statement in indexes:
        await session.run(statement)
    logger.info("Applied %s indexes for Book scoping", len(indexes))
