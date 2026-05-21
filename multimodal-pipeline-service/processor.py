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
    TransactionForFraudCheck, FraudDetectionResult # NEW
)

# In-memory store for task results (for demonstration purposes)
# In a real app, this would be a persistent store (e.g., Redis, database)
task_results: Dict[str, Dict[str, Any]] = {}

class MultimodalProcessor:
    def __init__(self):
        self.api_gateway_url = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

    async def _map_extracted_data_to_journal_entry(self, extracted_data: List[ExtractedField], source_context: Optional[str] = None) -> Optional[JournalEntryCreate]:
        amount = Decimal('0.00')
        description = source_context if source_context else "Multimodal input"
        entry_date = datetime.utcnow()
        reference_number = None

        for field in extracted_data:
            if field.field_name == "total_amount":
                try:
                    amount = Decimal(re.sub(r'[^\d.]', '', field.value))
                except Exception:
                    pass
            elif field.field_name == "date":
                try:
                    entry_date = datetime.fromisoformat(field.value.replace('Z', '+00:00'))
                except ValueError:
                    try:
                        entry_date = datetime.strptime(field.value, "%Y-%m-%d")
                    except ValueError:
                        pass
            elif field.field_name == "vendor":
                description = f"Expense from {field.value}"
            elif field.field_name == "transaction_description":
                description = field.value
            elif field.field_name == "reference_number":
                reference_number = field.value
            # More sophisticated mapping for account numbers would go here

        if amount == Decimal('0.00'):
            return None

        # Basic double-entry based on common expenses/income
        # These account numbers (6000, 1010, 4000, 1200) must exist in the Accounting Service's COA.
        lines: List[JournalLineBase] = []
        if "expense" in description.lower() or "receipt" in description.lower() or "bill" in description.lower():
            lines.append(JournalLineBase(account_number="6000", debit=amount, credit=Decimal('0.00'), description=description)) # Dummy Expense Account
            lines.append(JournalLineBase(account_number="1010", debit=Decimal('0.00'), credit=amount, description="Cash Payment")) # Dummy Cash Account
        elif "income" in description.lower() or "revenue" in description.lower() or "sale" in description.lower():
            lines.append(JournalLineBase(account_number="1010", debit=amount, credit=Decimal('0.00'), description="Cash Receipt")) # Dummy Cash Account
            lines.append(JournalLineBase(account_number="4000", debit=Decimal('0.00'), credit=amount, description=description)) # Dummy Revenue Account
        else:
            return None # Can't infer type of transaction

        total_debit = sum(line.debit for line in lines)
        total_credit = sum(line.credit for line in lines)
        if total_debit != total_credit:
            print(f"Proposed JE is unbalanced. Debit: {total_debit}, Credit: {total_credit}. Cannot create.")
            return None
        
        return JournalEntryCreate(
            entry_date=entry_date,
            description=description,
            reference_number=reference_number,
            source_module="Multimodal",
            lines=lines,
            status="pending" # Default to pending for automated entries
        )
        
    async def send_journal_entry_to_accounting_service(self, jwt_token: str, journal_entry: JournalEntryCreate) -> AutomatedJournalEntryResponse:
        headers = {
            "Authorization": f"Bearer {jwt_token}",
            "Content-Type": "application/json"
        }
        async with httpx.AsyncClient() as client:
            try:
                response = await client.post(
                    f"{self.api_gateway_url}/journal-entries/",
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

    async def process_document_ocr(self, image_data: Union[str, bytes], source_context: Optional[str] = None) -> DocumentParseResult:
        start_time = datetime.now()
        raw_text = ""
        extracted_data: List[ExtractedField] = []
        status = "failed"
        error_message = None

        try:
            await asyncio.sleep(2) # Simulate heavy OCR processing
            if isinstance(image_data, str) and image_data.startswith("http"):
                raw_text = "Simulated OCR from URL: Total: $123.45, Date: 2026-05-20, Vendor: ExampleCo"
            elif isinstance(image_data, str) and image_data.startswith("data:image/"):
                raw_text = "Simulated OCR from Base64: Total: $50.00, Date: 2026-05-19, Vendor: StoreXYZ"
            elif isinstance(image_data, bytes):
                raw_text = "Simulated OCR from bytes: Total: $75.20, Date: 2026-05-18, Vendor: Groceries"
            elif isinstance(image_data, str) and len(image_data) > 100: # Assuming base64 string from queue
                try:
                    # Dummy transaction ID for fraud check from multimodal input
                    temp_transaction_id = f"MM-{datetime.utcnow().timestamp()}-{hash(image_data) % 10000}"
                    raw_text = f"Simulated OCR from Base64 via Queue: Total: $75.20, Date: 2026-05-18, Vendor: QueuedGroceries. TransactionID:{temp_transaction_id}"
                except Exception:
                    raise ValueError("Invalid base64 image data.")
            else:
                raise ValueError("Unsupported image data format.")

            extracted_data.append(ExtractedField(field_name="total_amount", value="123.45", confidence=0.95))
            extracted_data.append(ExtractedField(field_name="date", value="2026-05-20", confidence=0.90))
            extracted_data.append(ExtractedField(field_name="vendor", value="ExampleCo", confidence=0.88))
            
            status = "completed"

        except Exception as e:
            error_message = str(e)
            print(f"OCR Simulation Error: {e}")
        
        processing_time_ms = int((datetime.now() - start_time).total_seconds() * 1000)

        return DocumentParseResult(
            document_type="receipt_or_invoice",
            extracted_data=extracted_data,
            raw_text=raw_text,
            status=status,
            processing_time_ms=processing_time_ms,
            error_message=error_message
        )

    async def process_audio_to_text(self, audio_data: Union[str, bytes], source_context: Optional[str] = None) -> AudioParseResult:
        start_time = datetime.now()
        transcription = ""
        extracted_entities: List[ExtractedField] = []
        status = "failed"
        error_message = None

        try:
            await asyncio.sleep(3) # Simulate heavy ASR processing
            if isinstance(audio_data, str) and audio_data.startswith("http"):
                transcription = "Simulated transcription from URL: Customer said amount is one hundred dollars to account number one zero one zero."
            elif isinstance(audio_data, bytes):
                transcription = "Simulated transcription from bytes: Record a payment of two hundred fifty to accounts payable."
            elif isinstance(audio_data, str) and len(audio_data) > 100: # Assuming base64 string from queue
                try:
                    # Dummy transaction ID for fraud check from multimodal input
                    temp_transaction_id = f"MM-{datetime.utcnow().timestamp()}-{hash(audio_data) % 10000}"
                    transcription = f"Simulated transcription from Base64 via Queue: Record a payment of three hundred to accounts receivable. TransactionID:{temp_transaction_id}"
                except Exception:
                    raise ValueError("Invalid base64 audio data.")
            else:
                raise ValueError("Unsupported audio data format.")

            extracted_entities.append(ExtractedField(field_name="amount", value="100.00", confidence=0.9))
            extracted_entities.append(ExtractedField(field_name="account_number", value="1010", confidence=0.85))

            status = "completed"

        except Exception as e:
            error_message = str(e)
            print(f"Audio Processing Simulation Error: {e}")

        processing_time_ms = int((datetime.now() - start_time).total_seconds() * 1000)

        return AudioParseResult(
            transcription=transcription,
            extracted_entities=extracted_entities,
            status=status,
            processing_time_ms=processing_time_ms,
            error_message=error_message
        )

    async def process_multimodal_input_sync(self, multimodal_input: MultimodalInput, jwt_token: Optional[str] = None) -> Union[DocumentParseResult, AudioParseResult, Dict[str, Any]]:
        """Synchronous processing of multimodal input (for direct API calls or fallback)."""
        if multimodal_input.input_type == "base64_image": # Handle base64 from queue
            return await self.process_document_ocr(multimodal_input.data, multimodal_input.source_context)
        elif multimodal_input.input_type == "base64_audio": # Handle base64 from queue
            return await self.process_audio_to_text(multimodal_input.data, multimodal_input.source_context)
        elif multimodal_input.input_type in ["image_url", "bytes_image"]: # Still need this path for actual bytes from file upload
            return await self.process_document_ocr(multimodal_input.data, multimodal_input.source_context)
        elif multimodal_input.input_type in ["audio_url", "bytes_audio"]: # Same here
            return await self.process_audio_to_text(multimodal_input.data, multimodal_input.source_context)
        elif multimodal_input.input_type == "text":
            await asyncio.sleep(0.3)
            extracted_data_from_text: List[ExtractedField] = []
            if "amount" in multimodal_input.data.lower():
                match = re.search(r'\$?(\d+\.?\d*)', multimodal_input.data)
                if match:
                    extracted_data_from_text.append(ExtractedField(field_name="total_amount", value=match.group(1)))
            if "account" in multimodal_input.data.lower():
                match = re.search(r'account number (\d+)', multimodal_input.data)
                if match:
                    extracted_data_from_text.append(ExtractedField(field_name="account_number", value=match.group(1)))

            je = await self._map_extracted_data_to_journal_entry(extracted_data_from_text, multimodal_input.data)
            
            return {
                "status": "completed",
                "extracted_text": multimodal_input.data,
                "proposed_journal_entry": je.model_dump(by_alias=True) if je else None,
                "message": "Text input processed."
            }
        else:
            return {"status": "failed", "error_message": "Unsupported input type.", "input_type": multimodal_input.input_type}

    async def process_multimodal_to_journal_entry_sync(self, multimodal_input: MultimodalInput, jwt_token: str) -> AutomatedJournalEntryResponse:
        """Synchronous processing and JE creation (for direct API calls or fallback)."""
        processed_result = await self.process_multimodal_input_sync(multimodal_input, jwt_token)
        
        extracted_data = []
        if isinstance(processed_result, DocumentParseResult):
            extracted_data = processed_result.extracted_data
        elif isinstance(processed_result, AudioParseResult):
            extracted_data = processed_result.extracted_entities if processed_result.extracted_entities else []
        elif isinstance(processed_result, dict) and processed_result.get("proposed_journal_entry"):
            proposed_je_dict = processed_result["proposed_journal_entry"]
            if proposed_je_dict:
                proposed_je = JournalEntryCreate(**proposed_je_dict)

                # NEW: Perform fraud detection before sending to accounting
                # For multimodal input, we need to create a dummy transaction_id
                transaction_id = f"MM-{datetime.utcnow().timestamp()}-{hash(multimodal_input.data) % 10000}"
                amount = Decimal('0.00')
                for item in extracted_data:
                    if item.field_name == "total_amount":
                        try:
                            amount = Decimal(re.sub(r'[^\d.]', '', item.value))
                        except Exception:
                            pass
                
                # If amount is not extracted, default to 1.00 for fraud check
                if amount == Decimal('0.00'): amount = Decimal('1.00')

                transaction_for_fraud = TransactionForFraudCheck(
                    transaction_id=transaction_id,
                    amount=amount,
                    currency="USD", # Default currency
                    sender_account_id="MultimodalSource",
                    recipient_account_id="MultimodalDestination",
                    transaction_type="unknown",
                    timestamp=proposed_je.entry_date if proposed_je else datetime.utcnow()
                )
                fraud_detection_result = await self._send_transaction_to_fraud_detection_service(jwt_token, transaction_for_fraud)

                je_response = await self.send_journal_entry_to_accounting_service(jwt_token, proposed_je)
                je_response.fraud_detection_result = fraud_detection_result # Attach fraud result

                return je_response
            else:
                return AutomatedJournalEntryResponse(
                    status="failed",
                    message="Text input processed but no valid journal entry could be proposed.",
                    extracted_data=processed_result
                )

        if not extracted_data:
            return AutomatedJournalEntryResponse(
                status="failed",
                message="No financial data could be extracted from the multimodal input.",
                extracted_data=processed_result
            )

        proposed_journal_entry = await self._map_extracted_data_to_journal_entry(
            extracted_data, multimodal_input.source_context
        )

        if not proposed_journal_entry:
            return AutomatedJournalEntryResponse(
                status="failed",
                message="Extracted data could not be mapped to a valid Journal Entry (e.g., missing amount, unbalanced).",
                extracted_data=processed_result
            )
        
        # NEW: Perform fraud detection
        transaction_id = f"MM-{datetime.utcnow().timestamp()}-{hash(multimodal_input.data) % 10000}"
        amount = Decimal('0.00')
        for item in extracted_data:
            if item.field_name == "total_amount":
                try:
                    amount = Decimal(re.sub(r'[^\d.]', '', item.value))
                except Exception:
                    pass
        if amount == Decimal('0.00'): amount = Decimal('1.00')

        transaction_for_fraud = TransactionForFraudCheck(
            transaction_id=transaction_id,
            amount=amount,
            currency="USD", # Default currency
            sender_account_id="MultimodalSource",
            recipient_account_id="MultimodalDestination",
            transaction_type="unknown",
            timestamp=proposed_journal_entry.entry_date if proposed_journal_entry else datetime.utcnow()
        )
        fraud_detection_result = await self._send_transaction_to_fraud_detection_service(jwt_token, transaction_for_fraud)

        je_response = await self.send_journal_entry_to_accounting_service(jwt_token, proposed_journal_entry)
        je_response.fraud_detection_result = fraud_detection_result # Attach fraud result

        return je_response
