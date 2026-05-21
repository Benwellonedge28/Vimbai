import asyncio
import os
from datetime import datetime
import re
from decimal import Decimal
from typing import Dict, Any, List, Union, Optional
import httpx
import base64

from multimodal_pipeline_service.models import (
    MultimodalInput, DocumentParseResult, ExtractedField, AudioParseResult,
    JournalLineBase, JournalEntryCreate, AutomatedJournalEntryResponse,
    TransactionForFraudCheck, FraudDetectionResult
)

# In-memory store for task results (for demonstration purposes)
# In a real app, this would be a persistent store (e.g., Redis, database)
task_results: Dict[str, Dict[str, Any]] = {}

class MultimodalProcessor:
    def __init__(self):
        self.api_gateway_url = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")
        self.accounting_service_url = f"{self.api_gateway_url}/accounting"

    async def _get_account_details(self, jwt_token: str, account_number: str) -> Optional[Dict]:
        """Fetches account details from the Accounting Service to validate account numbers."""
        headers = {"Authorization": f"Bearer {jwt_token}"}
        try:
            async with httpx.AsyncClient() as client:
                response = await client.get(
                    f"{self.accounting_service_url}/accounts/{account_number}",
                    headers=headers
                )
                response.raise_for_status()
                return response.json()
        except httpx.HTTPStatusError as e:
            print(f"Account {account_number} not found in Accounting Service: {e.response.status_code} - {e.response.text}")
            return None
        except httpx.RequestError as e:
            print(f"Network error fetching account {account_number} from Accounting Service: {e}")
            return None

    async def _map_extracted_data_to_journal_entry(self, extracted_data: List[ExtractedField], source_context: Optional[str] = None, jwt_token: str = None) -> Optional[JournalEntryCreate]:
        """
        Maps extracted data from multimodal input to a JournalEntryCreate object.
        Includes more sophisticated logic for account inference and validation.
        """
        # Default values
        total_amount = Decimal('0.00')
        invoice_date = datetime.utcnow()
        vendor_name = "Unknown Vendor"
        description = source_context if source_context else "Multimodal document processing"
        reference_number = None
        
        # --- Extract key fields ---
        for field in extracted_data:
            if field.field_name == "total_amount":
                try:
                    # Clean and convert to Decimal, handling currency symbols
                    total_amount = Decimal(re.sub(r'[^\\d.]', '', field.value))
                except Exception:
                    pass
            elif field.field_name == "invoice_date":
                try:
                    # Attempt various date formats
                    invoice_date = datetime.fromisoformat(field.value.replace('Z', '+00:00'))
                except ValueError:
                    try:
                        invoice_date = datetime.strptime(field.value, "%Y-%m-%d")
                    except ValueError:
                        try: # Another common format
                            invoice_date = datetime.strptime(field.value, "%m/%d/%Y")
                        except ValueError:
                            pass # Keep default if parsing fails
            elif field.field_name == "vendor_name":
                vendor_name = field.value
            elif field.field_name == "invoice_number":
                reference_number = field.value
            elif field.field_name == "description":
                description = field.value
        
        # If no amount was extracted, cannot create a journal entry
        if total_amount == Decimal('0.00'):
            return None

        description = f"{vendor_name} - {description}"
        if reference_number:
            description = f"Invoice {reference_number} from {description}"
        
        # --- Account Inference Logic ---
        # This is a critical point for intelligence. For a real system, this would involve:
        # 1. Machine learning model to classify transaction type (e.g., Office Supplies, Rent, Consulting Fee)
        # 2. Pre-defined user mapping rules (e.g., if vendor is "RentCo", always use Rent Expense account)
        # 3. LLM-based inference if classification is uncertain.
        # 4. Lookup from an accounting service endpoint for suggested accounts based on vendor/category.

        # For this implementation, we will use a more intelligent rule-based approach
        # and attempt to validate against a (hypothetical) accounting service endpoint for account suggestions
        # or fall back to dummy accounts if validation fails.

        # Hypothetical accounting service endpoint for account suggestion:
        # GET /accounting/accounts/suggest?category=...&vendor=...&amount=...
        # For now, we will stick to basic keywords and check against known dummy accounts.

        # Dummy Account Numbers (must exist in Accounting Service's COA for a real system)
        # These would ideally be configurable or inferred dynamically.
        ACCOUNTS_PAYABLE = "2000" # Liability
        CASH = "1010"             # Asset
        RENT_EXPENSE = "6100"     # Expense
        OFFICE_SUPPLIES_EXPENSE = "6200" # Expense
        SALES_REVENUE = "4000"   # Revenue
        # Add more as needed for different categories

        debit_account_number = None
        credit_account_number = None

        # Simple keyword-based inference for expense accounts
        lower_desc = description.lower()
        if "rent" in lower_desc:
            debit_account_number = RENT_EXPENSE
            credit_account_number = ACCOUNTS_PAYABLE # Assuming it's an AP bill
        elif "supplies" in lower_desc or "stationery" in lower_desc:
            debit_account_number = OFFICE_SUPPLIES_EXPENSE
            credit_account_number = ACCOUNTS_PAYABLE
        elif "invoice" in lower_desc: # General invoice from vendor
            debit_account_number = OFFICE_SUPPLIES_EXPENSE # Fallback for now, needs better inference
            credit_account_number = ACCOUNTS_PAYABLE
        elif "sale" in lower_desc or "revenue" in lower_desc: # Revenue transaction
            debit_account_number = CASH
            credit_account_number = SALES_REVENUE
        
        # Fallback if no specific account can be inferred
        if not debit_account_number or not credit_account_number:
            print("Could not infer specific accounts. Using general expense/cash.")
            debit_account_number = "6000" # Generic Expense
            credit_account_number = "1010" # Generic Cash/AP

        # Validate inferred accounts against Accounting Service if JWT is available
        if jwt_token:
            debit_acc_details = await self._get_account_details(jwt_token, debit_account_number)
            credit_acc_details = await self._get_account_details(jwt_token, credit_account_number)
            if not debit_acc_details or not credit_acc_details:
                print(f"Inferred account numbers not found in Accounting Service. Debit: {debit_account_number}, Credit: {credit_account_number}. Using pending status.")
                # We can decide to either return None, or proceed with inferred and set status to 'needs_review'
                # For now, we'll proceed but acknowledge it's not fully validated.
        
        lines: List[JournalLineBase] = []
        # Assuming all transactions from multimodal input are either expenses (debited to expense, credited to AP/Cash)
        # or revenues (debited to Cash, credited to Revenue)
        if debit_account_number and credit_account_number:
            lines.append(JournalLineBase(account_number=debit_account_number, debit=total_amount, credit=Decimal('0.00'), description=description))
            lines.append(JournalLineBase(account_number=credit_account_number, debit=Decimal('0.00'), credit=total_amount, description=description))
        else:
            return None # Failed to infer balanced accounts

        total_debit = sum(line.debit for line in lines)
        total_credit = sum(line.credit for line in lines)
        if total_debit != total_credit:
            print(f"Proposed JE is unbalanced. Debit: {total_debit}, Credit: {total_credit}. Cannot create.")
            return None
        
        return JournalEntryCreate(
            entry_date=invoice_date,
            description=description,
            reference_number=reference_number,
            source_module="Multimodal",
            lines=lines,
            status="pending" # Automated entries should be pending for review
        )
        
    async def send_journal_entry_to_accounting_service(self, jwt_token: str, journal_entry: JournalEntryCreate) -> AutomatedJournalEntryResponse:
        headers = {
            "Authorization": f"Bearer {jwt_token}",
            "Content-Type": "application/json"
        }
        async with httpx.AsyncClient() as client:
            try:
                response = await client.post(
                    f"{self.accounting_service_url}/journal-entries/", # Updated to accounting_service_url
                    headers=headers,
                    json=journal_entry.model_dump(by_alias=True)
                )
                response.raise_for_status() # Raise HTTPStatusError for bad responses (4xx or 5xx)
                return AutomatedJournalEntryResponse(
                    status="success",
                    message="Journal entry created successfully.",
                    journal_entry_id=response.json().get("id"),
                    proposed_journal_entry=journal_entry
                )
            except httpx.HTTPStatusError as e:
                print(f"HTTP error creating JE: {e.response.status_code} - {e.response.text}")
                return AutomatedJournalEntryResponse(
                    status="failed",
                    message=f"Failed to create journal entry in Accounting Service (HTTP {e.response.status_code}): {e.response.text}",
                    proposed_journal_entry=journal_entry
                )
            except httpx.RequestError as e:
                print(f"Network error creating JE: {e}")
                return AutomatedJournalEntryResponse(
                    status="failed",
                    message=f"Network error connecting to Accounting Service: {e}",
                    proposed_journal_entry=journal_entry
                )

    async def _send_transaction_to_fraud_detection_service(self, jwt_token: str, transaction_data: TransactionForFraudCheck) -> FraudDetectionResult:
        """Internal helper to send transaction to Fraud Detection Service."""
        headers = {
            "Authorization": f"Bearer {jwt_token}",
            "Content-Type": "application/json"
        }
        async with httpx.AsyncClient() as client:
            try:
                response = await client.post(
                    f"{self.api_gateway_url}/fraud-detection/analyze-transaction/",
                    headers=headers,
                    json=transaction_data.model_dump(by_alias=True)
                )
                response.raise_for_status()
                return FraudDetectionResult(**response.json())
            except httpx.HTTPStatusError as e:
                error_detail = e.response.json().get("detail", e.response.text)
                print(f"Fraud Detection Service error: {error_detail}")
                return FraudDetectionResult(
                    transaction_id=transaction_data.transaction_id,
                    fraud_score=0.0,
                    fraud_flag="safe", # Default to safe if service fails
                    reason=f"Fraud detection service failed: {error_detail}",
                    model_version="N/A_service_unavailable"
                )
            except httpx.RequestError as e:
                print(f"Network error communicating with Fraud Detection Service: {e}")
                return FraudDetectionResult(
                    transaction_id=transaction_data.transaction_id,
                    fraud_score=0.0,
                    fraud_flag="safe", # Default to safe if service unavailable
                    reason=f"Network error connecting to Fraud Detection Service: {e}",
                    model_version="N/A_network_error"
                )

    async def process_document_ocr(self, image_data: Union[str, bytes], source_context: Optional[str] = None, jwt_token: str = None) -> DocumentParseResult:
        """
        Simulates OCR processing and extracts data from an image/document.
        Now returns more structured and realistic simulated data.
        """
        start_time = datetime.now()
        raw_text = ""
        extracted_data: List[ExtractedField] = []
        status_str = "failed" # Renamed to avoid conflict with `status` variable in outer scope
        error_message = None

        # --- Placeholder for actual OCR library integration ---
        # In a real-world scenario, you would integrate with an actual OCR library or service here.
        # Example using a hypothetical OCR_Client:
        # from ocr_library import OCR_Client
        # ocr_client = OCR_Client()
        # raw_ocr_output = await ocr_client.process_image(image_data)
        # raw_text = raw_ocr_output.full_text

        # For this simulation, we'll generate structured data based on input type.
        if isinstance(image_data, str) and image_data.startswith("http"):
            raw_text = "Simulated OCR from URL."
            # Simulate a parsed invoice structure
            extracted_data = [
                ExtractedField(field_name="vendor_name", value="Cloud Services Inc.", confidence=0.98),
                ExtractedField(field_name="invoice_number", value="INV-2026-00123", confidence=0.97),
                ExtractedField(field_name="invoice_date", value="2026-05-15", confidence=0.95),
                ExtractedField(field_name="total_amount", value="1500.75", confidence=0.99),
                ExtractedField(field_name="description", value="Monthly cloud hosting", confidence=0.90),
            ]
        elif isinstance(image_data, str) and image_data.startswith("data:image/"):
            raw_text = "Simulated OCR from Base64."
            extracted_data = [
                ExtractedField(field_name="vendor_name", value="Office Supplies Co.", confidence=0.95),
                ExtractedField(field_name="invoice_number", value="PO-98765", confidence=0.90),
                ExtractedField(field_name="invoice_date", value="2026-05-10", confidence=0.92),
                ExtractedField(field_name="total_amount", value="250.30", confidence=0.96),
                ExtractedField(field_name="description", value="Pens, paper, toner", confidence=0.85),
            ]
        elif isinstance(image_data, bytes):
            raw_text = "Simulated OCR from Bytes."
            extracted_data = [
                ExtractedField(field_name="vendor_name", value="Utilities Provider", confidence=0.90),
                ExtractedField(field_name="invoice_date", value="2026-05-01", confidence=0.88),
                ExtractedField(field_name="total_amount", value="85.50", confidence=0.93),
                ExtractedField(field_name="description", value="Electricity bill", confidence=0.80),
            ]
        else:
            error_message = "Unsupported image data format."
            print(f"OCR Simulation Error: {error_message}")
            return DocumentParseResult(
                extracted_data=extracted_data,
                raw_text=raw_text,
                status="failed",
                error_message=error_message,
                processing_time=(datetime.now() - start_time).total_seconds()
            )

        status_str = "completed"

        # --- Intelligent mapping to Journal Entry after extraction ---
        # This part remains conceptually similar, but now with more refined inputs.
        # The _map_extracted_data_to_journal_entry will now perform more intelligent inference.
        proposed_journal_entry = await self._map_extracted_data_to_journal_entry(extracted_data, source_context, jwt_token)
        automated_je_response = None
        if proposed_journal_entry:
            # Send to accounting service
            automated_je_response = await self.send_journal_entry_to_accounting_service(jwt_token, proposed_journal_entry)
            if automated_je_response.status == "success":
                status_str = "processed_and_posted"
            else:
                status_str = "processed_with_je_error"
                error_message = automated_je_response.message
        else:
            status_str = "extracted_data_no_je"
            error_message = "Could not infer a balanced journal entry from extracted data."

        return DocumentParseResult(
            extracted_data=extracted_data,
            raw_text=raw_text,
            status=status_str,
            error_message=error_message,
            processing_time=(datetime.now() - start_time).total_seconds(),
            proposed_journal_entry_response=automated_je_response
        )

    async def process_audio_asr(self, audio_data: Union[str, bytes], source_context: Optional[str] = None) -> AudioParseResult:
        start_time = datetime.now()
        transcribed_text = ""
        extracted_commands: List[str] = []
        status_str = "failed"
        error_message = None

        # Simulate ASR and command extraction
        await asyncio.sleep(1.5) # Simulate ASR processing
        if isinstance(audio_data, str) and audio_data.startswith("http"):
            transcribed_text = "Simulated ASR from URL: Create an expense for $50 for coffee."
        elif isinstance(audio_data, str) and audio_data.startswith("data:audio/"):
            transcribed_text = "Simulated ASR from Base64: Record income of $200 from consulting."
        elif isinstance(audio_data, bytes):
            transcribed_text = "Simulated ASR from bytes: What is my current cash balance?"
        else:
            error_message = "Unsupported audio data format."
            print(f"ASR Simulation Error: {error_message}")
            return AudioParseResult(
                transcribed_text=transcribed_text,
                extracted_commands=[],
                status="failed",
                error_message=error_message,
                processing_time=(datetime.now() - start_time).total_seconds()
            )

        # Simple command extraction
        if "expense" in transcribed_text.lower() and "$" in transcribed_text:
            match = re.search(r'\$?(\d+\.?\d*)', transcribed_text)
            if match:
                extracted_commands.append(f"create_expense_journal_entry_amount_{match.group(1)}")
        elif "income" in transcribed_text.lower() and "$" in transcribed_text:
            match = re.search(r'\$?(\d+\.?\d*)', transcribed_text)
            if match:
                extracted_commands.append(f"create_income_journal_entry_amount_{match.group(1)}")
        elif "balance" in transcribed_text.lower():
            extracted_commands.append("get_cash_balance")

        status_str = "completed"

        return AudioParseResult(
            transcribed_text=transcribed_text,
            extracted_commands=extracted_commands,
            status=status_str,
            error_message=error_message,
            processing_time=(datetime.now() - start_time).total_seconds()
        )

# --- Task management functions (for demonstration) ---
async def enqueue_task(task_id: str, processor: MultimodalProcessor, input_data: MultimodalInput, jwt_token: str = None):
    # In a real system, this would push to a message queue (e.g., Kafka, RabbitMQ)
    # and a worker would pick it up.
    # For demonstration, we'll run it directly.
    task_results[task_id] = {"status": "processing", "progress": 0, "result": None, "error": None}
    try:
        if input_data.document_data:
            result = await processor.process_document_ocr(input_data.document_data, input_data.source_context, jwt_token)
        elif input_data.audio_data:
            result = await processor.process_audio_asr(input_data.audio_data, input_data.source_context)
        else:
            raise ValueError("No document or audio data provided.")
        task_results[task_id]["result"] = result.model_dump()
        task_results[task_id]["status"] = "completed"
        task_results[task_id]["progress"] = 100
    except Exception as e:
        task_results[task_id]["error"] = str(e)
        task_results[task_id]["status"] = "failed"
        task_results[task_id]["progress"] = 100

async def get_task_status(task_id: str) -> Dict[str, Any]:
    return task_results.get(task_id, {"status": "not_found", "progress": 0})
