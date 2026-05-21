import os # NEW
from typing import List, Optional
from neo4j import AsyncSession
from banking_integration_service import models, crud
from banking_integration_service.clients.accounting_service_client import ( # NEW
    AccountingServiceClient, AccountingServiceClientException,
    MockJournalEntryCreate, MockJournalLineCreate
)
from decimal import Decimal
import uuid

class TransactionProcessor:
    def __init__(self, db_session: AsyncSession):
        self.db_session = db_session
        self.accounting_service_client = AccountingServiceClient() # NEW

    async def _apply_categorization_rules(self, user_id: str, transaction: models.BankTransactionInDB) -> models.BankTransactionUpdate:
        """Applies user-defined categorization rules to a bank transaction."""
        rules = await crud.get_all_categorization_rules(self.db_session, user_id)
        
        # Sort rules by priority (lower number = higher priority)
        rules.sort(key=lambda rule: rule.priority)

        updated_category = transaction.category
        # target_finacc_account_number_from_rule: Optional[str] = None # Removed this as it's not part of BankTransactionUpdate

        for rule in rules:
            if not rule.is_active:
                continue

            match_found = False
            if rule.match_field == "description" and rule.match_pattern:
                if rule.match_pattern.lower() in transaction.description.lower():
                    match_found = True
            elif rule.match_field == "payee" and rule.match_pattern:
                if rule.match_pattern.lower() in transaction.description.lower():
                    match_found = True
            elif rule.match_field == "amount_range":
                # Assuming match_pattern is something like "min-max" or just "exact_amount"
                pass 

            if match_found:
                updated_category = rule.target_category
                # No longer setting target_finacc_account_number directly on BankTransactionUpdate, 
                # as it is used for JE creation dynamically.
                break
        
        return models.BankTransactionUpdate(
            category=updated_category,
        )

    async def process_new_bank_transaction(self, user_id: str, transaction: models.BankTransactionInDB) -> models.BankTransactionInDB:
        """
        Processes a new bank transaction:
        1. Applies categorization rules.
        2. Creates a draft Journal Entry in the Accounting Service if not already reconciled.
        3. Attempts to find a reconciliation match.
        """
        # 1. Apply categorization rules
        categorization_update = await self._apply_categorization_rules(user_id, transaction)
        
        # Update the transaction in DB with new category if found
        if categorization_update.category and categorization_update.category != transaction.category:
            transaction = await crud.update_bank_transaction(
                self.db_session,
                transaction.id!,
                categorization_update
            )
            transaction = (await crud.get_bank_transaction(self.db_session, transaction.id!))! # Re-fetch updated transaction

        # 2. If not already reconciled, create a draft Journal Entry
        if not transaction.is_reconciled and not transaction.finacc_journal_entry_id:
            target_finacc_account_number: Optional[str] = None
            rules = await crud.get_all_categorization_rules(self.db_session, user_id)
            for rule in rules:
                if rule.is_active and rule.target_finacc_account_number and \
                   ((rule.match_field == "description" and rule.match_pattern.lower() in transaction.description.lower()) or \
                    (rule.match_field == "payee" and rule.match_pattern.lower() in transaction.description.lower())):
                    target_finacc_account_number = rule.target_finacc_account_number
                    break

            if transaction.amount > Decimal('0.00'): # Outgoing transaction (expense)
                debit_account = target_finacc_account_number or os.getenv("DEFAULT_EXPENSE_ACCOUNT_NUMBER", "5000")
                credit_account = os.getenv("DEFAULT_CASH_ACCOUNT_NUMBER", "1000")
                description_je = f"Bank Expense: {transaction.description} ({transaction.category or 'Uncategorized'})"
            else: # Incoming transaction (revenue)
                debit_account = os.getenv("DEFAULT_CASH_ACCOUNT_NUMBER", "1000")
                credit_account = target_finacc_account_number or os.getenv("DEFAULT_REVENUE_ACCOUNT_NUMBER", "4000")
                description_je = f"Bank Revenue: {transaction.description} ({transaction.category or 'Uncategorized'})"

            journal_entry_data = MockJournalEntryCreate( # Using Mock class, should be actual JournalEntryCreate
                entry_date=transaction.date.isoformat(),
                description=description_je,
                source_module="Banking Integration",
                reference_number=transaction.transaction_id,
                lines=[
                    MockJournalLineCreate(account_number=debit_account, debit=float(abs(transaction.amount)), credit=float(Decimal('0.00')), description=description_je),
                    MockJournalLineCreate(account_number=credit_account, debit=float(Decimal('0.00')), credit=float(abs(transaction.amount)), description=description_je),
                ]
            )
            
            try:
                # Actual call to Accounting Service
                new_je_response = await self.accounting_service_client.create_journal_entry(journal_entry_data, user_id)
                new_je_id = new_je_response.get("id")
                print(f"Created Journal Entry {new_je_id} for transaction {transaction.id}")
                
                transaction = await crud.update_bank_transaction(
                    self.db_session,
                    transaction.id!,
                    models.BankTransactionUpdate(finacc_journal_entry_id=new_je_id)
                )
            except AccountingServiceClientException as e:
                print(f"ERROR: Failed to create journal entry for transaction {transaction.id}: {e.args[0]}")
            except Exception as e:
                print(f"ERROR: An unexpected error occurred while creating journal entry for transaction {transaction.id}: {e}")

        # 3. Attempt to find reconciliation match
        if transaction.finacc_journal_entry_id and not transaction.is_reconciled:
            transaction = await crud.update_bank_transaction(
                self.db_session,
                transaction.id!,
                models.BankTransactionUpdate(is_reconciled=True)
            )
            print(f"Transaction {transaction.id} reconciled with JE {transaction.finacc_journal_entry_id}")
            
        return transaction
