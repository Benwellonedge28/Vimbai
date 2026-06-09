import uuid
from typing import List, Dict, Any, Optional
from datetime import datetime, date, timezone, timedelta
from neo4j import AsyncSession
from banking_integration_service import crud, models
# from accounting_service.crud import get_journal_entry_by_id # Assuming accounting_service.crud is available
from decimal import Decimal

# Placeholder for NotFoundError from common exceptions or specific service exceptions
class NotFoundError(Exception):
    def __init__(self, detail: str, status_code: int = 404):
        self.detail = detail
        self.status_code = status_code

class ReconciliationEngineException(Exception):
    """Custom exception for reconciliation engine errors."""
    pass

class ReconciliationEngine:
    def __init__(self, db_session: AsyncSession):
        self.db_session = db_session

    async def _find_potential_matches_for_transaction(self, bank_transaction: models.BankTransactionInDB) -> List[models.ReconciliationMatchInDB]:
        """
        Finds potential matching Journal Entries for a given Bank Transaction.
        Uses heuristics like amount, date, and keywords in description.
        """
        potential_matches: List[models.ReconciliationMatchInDB] = []

        # Heuristic 1: Exact amount and close date
        # Search for Journal Entries with a matching amount (absolute value)
        # and a date within a small window (e.g., +/- 3 days)
        # Note: We need access to Accounting Service's Journal Entry models and CRUD
        # For now, let's mock this by assuming we can query JEs.
        
        # This part assumes a get_journal_entries_by_amount_and_date_range in accounting_service.crud
        # For now, we'll just mock it.
        mock_journal_entries = [
            # Example: A journal entry that matches the bank transaction amount
            # In a real system, these would be fetched from the accounting service
            models.JournalEntryInDB(
                id="je-123-mock",
                entryDate=bank_transaction.date, # Using bank transaction date for mock match
                description="Mock JE for bank transaction",
                sourceModule="Banking",
                lines=[], # Simplified, real JE has lines
                createdAt=datetime.now(timezone.utc),
                updatedAt=datetime.now(timezone.utc),
            )
        ]

        for je_in_db in mock_journal_entries: # Iterate through actual JEs from accounting service
            # For simplicity, we are looking for a JE with total debit/credit equal to bank transaction amount
            # In a real system, we'd sum up journal lines for JE total amount.
            je_total_amount = Decimal(abs(bank_transaction.amount)) # Simplified assumption
            
            # Date proximity (within 3 days)
            date_diff = abs((bank_transaction.date - je_in_db.entryDate).days)

            if je_total_amount == abs(bank_transaction.amount) and date_diff <= 3:
                match_id = str(uuid.uuid4())
                potential_matches.append(models.ReconciliationMatchInDB(
                    id=match_id,
                    bank_transaction_id=bank_transaction.id,
                    finacc_journal_entry_id=je_in_db.id,
                    match_type="fuzzy", # Could be "exact" if all criteria are very strict
                    matched_amount=abs(bank_transaction.amount),
                    matched_date=je_in_db.entryDate,
                    is_confirmed=False,
                    createdAt=datetime.now(timezone.utc),
                    updatedAt=datetime.now(timezone.utc),
                ))
        return potential_matches

    async def get_transactions_for_reconciliation(self, user_id: str, account_id: Optional[str] = None) -> List[models.BankTransactionInDB]:
        """
        Retrieves bank transactions that are not yet reconciled.
        """
        # This would ideally query all transactions for a user or account that have is_reconciled = false
        # For now, we'll use a mock list
        mock_unreconciled_transactions = [
            models.BankTransactionInDB(
                id="bt-1-unreconciled",
                transaction_id="plaid-txn-123",
                account_id="ba-1-mock",
                description="Walmart Supercenter",
                amount=Decimal("-55.23"),
                date=date(2026, 5, 18),
                posted_date=date(2026, 5, 19),
                is_reconciled=False,
                currency="USD",
                createdAt=datetime.now(timezone.utc),
                updatedAt=datetime.now(timezone.utc),
            ),
             models.BankTransactionInDB(
                id="bt-2-unreconciled",
                transaction_id="plaid-txn-124",
                account_id="ba-1-mock",
                description="Netflix Subscription",
                amount=Decimal("-15.99"),
                date=date(2026, 5, 20),
                posted_date=date(2026, 5, 20),
                is_reconciled=False,
                currency="USD",
                createdAt=datetime.now(timezone.utc),
                updatedAt=datetime.now(timezone.utc),
            )
        ]
        return [t for t in mock_unreconciled_transactions if not t.is_reconciled]


    async def suggest_reconciliations(self, user_id: str, account_id: Optional[str] = None) -> Dict[str, List[models.ReconciliationMatchInDB]]:
        """
        Suggests potential reconciliations for unreconciled bank transactions.
        """
        unreconciled_transactions = await self.get_transactions_for_reconciliation(user_id, account_id)
        suggestions: Dict[str, List[models.ReconciliationMatchInDB]] = {}

        for transaction in unreconciled_transactions:
            matches = await self._find_potential_matches_for_transaction(transaction)
            if matches:
                suggestions[transaction.id] = matches
        return suggestions

    async def confirm_reconciliation_match(self, match_id: str, confirmed_by_user_id: str) -> models.ReconciliationMatchInDB:
        """
        Confirms a suggested reconciliation match.
        Also updates the corresponding BankTransaction to 'is_reconciled = True'.
        """
        reconciliation_match = await crud.get_reconciliation_match(self.db_session, match_id)
        if not reconciliation_match:
            raise NotFoundError(f"Reconciliation Match {match_id} not found.")

        # Update the match status
        updated_match = await crud.update_reconciliation_match(self.db_session, match_id, models.ReconciliationMatchUpdate(
            is_confirmed=True,
            confirmed_by_user_id=confirmed_by_user_id,
            confirmed_at=datetime.now(timezone.utc)
        ))
        if not updated_match:
            raise ReconciliationEngineException(f"Failed to update reconciliation match {match_id}.")

        # Update the BankTransaction as reconciled
        await crud.update_bank_transaction(self.db_session, reconciliation_match.bank_transaction_id, models.BankTransactionUpdate(
            is_reconciled=True
        ))
        return updated_match

    async def create_manual_reconciliation(self, bank_transaction_id: str, finacc_journal_entry_id: str, user_id: str) -> models.ReconciliationMatchInDB:
        """
        Creates a manual reconciliation match between a bank transaction and a journal entry.
        """
        # Verify bank transaction exists and is not reconciled
        bank_transaction = await crud.get_bank_transaction(self.db_session, bank_transaction_id)
        if not bank_transaction:
            raise NotFoundError(f"Bank Transaction {bank_transaction_id} not found.")
        if bank_transaction.is_reconciled:
            raise ReconciliationEngineException(f"Bank Transaction {bank_transaction_id} is already reconciled.")

        # Verify journal entry exists
        # This requires calling accounting service's crud, which is not directly available here.
        # For now, we will assume get_journal_entry_by_id fetches it.
        # je = await get_journal_entry_by_id(self.db_session, finacc_journal_entry_id)
        # if not je:
        #     raise NotFoundError(f"Journal Entry {finacc_journal_entry_id} not found.")

        # Create the manual match
        manual_match_data = models.ReconciliationMatchCreate(
            bank_transaction_id=bank_transaction_id,
            finacc_journal_entry_id=finacc_journal_entry_id,
            match_type="manual",
            matched_amount=abs(bank_transaction.amount), # Assuming full amount match for manual
            matched_date=bank_transaction.date,
            is_confirmed=True,
            confirmed_by_user_id=user_id,
            confirmed_at=datetime.now(timezone.utc)
        )
        new_match = await crud.create_reconciliation_match(self.db_session, manual_match_data)

        # Update the BankTransaction as reconciled
        await crud.update_bank_transaction(self.db_session, bank_transaction_id, models.BankTransactionUpdate(
            is_reconciled=True,
            finacc_journal_entry_id=finacc_journal_entry_id # Link it here too for easier lookup
        ))
        return new_match
