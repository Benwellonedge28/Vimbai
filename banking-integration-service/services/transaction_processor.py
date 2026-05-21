from typing import List, Optional
from neo4j import AsyncSession
from banking_integration_service import models, crud
# from accounting_service.models import JournalEntryCreate, JournalLineCreate # Assuming accounting models are accessible
# from accounting_service.crud import create_journal_entry # Assuming accounting CRUD is accessible
from decimal import Decimal
import uuid # For mock JE ID

class TransactionProcessor:
    def __init__(self, db_session: AsyncSession):
        self.db_session = db_session

    async def _apply_categorization_rules(self, user_id: str, transaction: models.BankTransactionInDB) -> models.BankTransactionUpdate:
        """Applies user-defined categorization rules to a bank transaction."""
        rules = await crud.get_all_categorization_rules(self.db_session, user_id)
        
        # Sort rules by priority (lower number = higher priority)
        rules.sort(key=lambda rule: rule.priority)

        updated_category = transaction.category
        target_finacc_account_number_from_rule: Optional[str] = None

        for rule in rules:
            if not rule.is_active:
                continue

            match_found = False
            if rule.match_field == "description" and rule.match_pattern:
                if rule.match_pattern.lower() in transaction.description.lower():
                    match_found = True
            elif rule.match_field == "payee" and rule.match_pattern:
                # Plaid transactions don't always have a distinct 'payee' field directly.
                # Often it's part of the description or extracted from merchant_name.
                # For this simple example, we'll check in description.
                if rule.match_pattern.lower() in transaction.description.lower():
                    match_found = True
            elif rule.match_field == "amount_range":
                # Assuming match_pattern is something like "min-max" or just "exact_amount"
                # This would require more sophisticated parsing of match_pattern
                pass # Skipping for now, more complex rule logic.

            if match_found:
                updated_category = rule.target_category
                if rule.target_finacc_account_number:
                    target_finacc_account_number_from_rule = rule.target_finacc_account_number
                # Apply only the first matching rule based on priority
                break
        
        return models.BankTransactionUpdate(
            category=updated_category,
            # For now, target_finacc_account_number is passed through to JE creation, not stored on BT itself.
            # If it needs to be stored on BT, models.BankTransactionInDB needs to be extended.
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
                categorization_update # Use the update model
            )
            # Re-fetch the transaction to ensure all fields are up-to-date
            transaction = (await crud.get_bank_transaction(self.db_session, transaction.id!))!

        # 2. If not already reconciled, create a draft Journal Entry
        if not transaction.is_reconciled and not transaction.finacc_journal_entry_id:
            # Determine debit/credit for JE
            # For simplicity: positive amount = expense (debit to expense, credit to cash)
            # negative amount = revenue (debit to cash, credit to revenue)
            # This logic needs to be more robust for different account types.
            # The `target_finacc_account_number_from_rule` would be retrieved from `_apply_categorization_rules`
            # For now, using placeholders directly.
            target_finacc_account_number: Optional[str] = None # Should come from categorization_update
            # Example: Retrieve rule again to get account number
            rules = await crud.get_all_categorization_rules(self.db_session, user_id)
            for rule in rules:
                if rule.is_active and rule.target_finacc_account_number and \n                   ((rule.match_field == "description" and rule.match_pattern.lower() in transaction.description.lower()) or \n                    (rule.match_field == "payee" and rule.match_pattern.lower() in transaction.description.lower())):\n                    target_finacc_account_number = rule.target_finacc_account_number
                    break

            if transaction.amount > Decimal('0.00'): # Outgoing transaction (expense)
                debit_account = target_finacc_account_number or os.getenv("DEFAULT_EXPENSE_ACCOUNT_NUMBER", "5000") # Placeholder Expense Account
                credit_account = os.getenv("DEFAULT_CASH_ACCOUNT_NUMBER", "1000") # Placeholder Cash Account
                description_je = f"Bank Expense: {transaction.description} ({transaction.category or 'Uncategorized'})"
            else: # Incoming transaction (revenue)
                debit_account = os.getenv("DEFAULT_CASH_ACCOUNT_NUMBER", "1000") # Placeholder Cash Account
                credit_account = target_finacc_account_number or os.getenv("DEFAULT_REVENUE_ACCOUNT_NUMBER", "4000") # Placeholder Revenue Account
                description_je = f"Bank Revenue: {transaction.description} ({transaction.category or 'Uncategorized'})"

            # Create JournalEntryCreate object - MOCKED for now
            # journal_entry_data = JournalEntryCreate(
            #     entry_date=transaction.date,
            #     description=description_je,
            #     source_module="Banking Integration",
            #     reference_number=transaction.transaction_id,
            #     lines=[
            #         JournalLineCreate(account_number=debit_account, debit=abs(transaction.amount), credit=Decimal('0.00'), description=description_je),
            #         JournalLineCreate(account_number=credit_account, debit=Decimal('0.00'), credit=abs(transaction.amount), description=description_je),
            #     ]
            # )
            
            try:
                # In a real microservice architecture, this would be an HTTP call to the Accounting Service
                # For now, we mock the creation of a Journal Entry ID
                new_je_id = str(uuid.uuid4())
                print(f"MOCK: Created Journal Entry {new_je_id} for transaction {transaction.id}")
                
                # Update bank transaction with new JE ID
                transaction = await crud.update_bank_transaction(
                    self.db_session,
                    transaction.id!,
                    models.BankTransactionUpdate(finacc_journal_entry_id=new_je_id)
                )
            except Exception as e:
                print(f"ERROR: Failed to create MOCKED journal entry for transaction {transaction.id}: {e}")

        # 3. Attempt to find reconciliation match
        # For simplicity, if we just created a JE, we can assume it's matched.
        # This would also create a ReconciliationMatch entity in a full implementation.
        if transaction.finacc_journal_entry_id and not transaction.is_reconciled:
            transaction = await crud.update_bank_transaction(
                self.db_session,
                transaction.id!,
                models.BankTransactionUpdate(is_reconciled=True)
            )
            print(f"Transaction {transaction.id} reconciled with JE {transaction.finacc_journal_entry_id}")
            
        return transaction
