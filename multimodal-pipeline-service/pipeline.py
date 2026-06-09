import base64
from typing import Union, List, Dict, Any
from io import BytesIO
import asyncio
from datetime import datetime
import re # For basic NLP
import httpx # For making HTTP calls to other services
import os
from decimal import Decimal

from multimodal_pipeline_service.models import (
    MultimodalInput, DocumentParseResult, ExtractedField, AudioParseResult,
    JournalLineBase, JournalEntryCreate, AutomatedJournalEntryResponse
)

class MultimodalPipeline:
    def __init__(self):
        self.api_gateway_url = os.getenv("API_GATEWAY_URL", "http://localhost:8081")

    async def _map_extracted_data_to_journal_entry(self, extracted_data: List[ExtractedField], source_context: Optional[str] = None) -> Optional[JournalEntryCreate]:
        # This is a basic mapping logic. A real system would use a more sophisticated NLU/ontology.
        amount = Decimal('0.00')
        description = source_context if source_context else "Multimodal input"
        entry_date = datetime.now(timezone.utc)
        reference_number = None

        for field in extracted_data:
            if field.field_name == "total_amount":
                try:
                    amount = Decimal(re.sub(r'[^\d.]', '', field.value)) # Remove non-numeric except dot
                except Exception:
                    pass
            elif field.field_name == "date":
                try:
                    entry_date = datetime.fromisoformat(field.value.replace('Z', '+00:00')) # Handle ISO format
                except ValueError:
                    try: # Try common date formats
                        entry_date = datetime.strptime(field.value, "%Y-%m-%d")
                    except ValueError:
                        pass
            elif field.field_name == "vendor":
                description = f"Expense from {field.value}"
            elif field.field_name == "account_number":
                # This implies direct account tagging, which is advanced. For initial, map to a generic.
                pass 
            elif field.field_name == "transaction_description":
                description = field.value
            elif field.field_name == "reference_number":
                reference_number = field.value

        if amount == Decimal('0.00'):
            return None # Cannot create JE without an amount

        # Basic double-entry based on common expenses/income. Highly simplified.
        # In a real system, this would involve account matching rules or LLM inference.
        lines: List[JournalLineBase] = []
        if "expense" in description.lower() or "receipt" in description.lower() or "bill" in description.lower():
            # Assume an expense: Debit an expense account, Credit Cash/Accounts Payable
            # Need a way to determine specific expense account (e.g., via NLU or user config)
            # For POC, use dummy accounts that should exist in the COA
            lines.append(JournalLineBase(account_number="6000", debit=amount, credit=Decimal('0.00'), description=description)) # Dummy Expense Account
            lines.append(JournalLineBase(account_number="1010", debit=Decimal('0.00'), credit=amount, description="Cash Payment")) # Dummy Cash Account
        elif "income" in description.lower() or "revenue" in description.lower() or "sale" in description.lower():
            # Assume income: Debit Cash/Accounts Receivable, Credit Revenue Account
            lines.append(JournalLineBase(account_number="1010", debit=amount, credit=Decimal('0.00'), description="Cash Receipt")) # Dummy Cash Account
            lines.append(JournalLineBase(account_number="4000", debit=Decimal('0.00'), credit=amount, description=description)) # Dummy Revenue Account
        else:
            return None # Can't infer type of transaction

        # Ensure the entry is balanced
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
            lines=lines
        )
    
    async def send_journal_entry_to_accounting_service(self, jwt_token: str, journal_entry: JournalEntryCreate) -> AutomatedJournalEntryResponse:
        headers = {
            "Authorization": f"Bearer {jwt_token}",
            "Content-Type": "application/json"
        }
        # Use httpx for async requests. Point to API Gateway which will route to Accounting Service.
        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{self.api_gateway_url}/journal-entries/",
                headers=headers,
                json=journal_entry.model_dump(by_alias=True) # Use model_dump for Pydantic v2
            )
        
        if response.status_code == 201:
            return AutomatedJournalEntryResponse(
                status="success",
                message="Journal entry created successfully.",
                journal_entry_id=response.json().get("id"),
                proposed_journal_entry=journal_entry # Include the entry that was created
            )
        else:
            return AutomatedJournalEntryResponse(
                status="failed",
                message=f"Failed to create journal entry in Accounting Service: {response.text}",
                proposed_journal_entry=journal_entry # Include the entry that was attempted
            )


    async def process_document_ocr(self, image_data: Union[str, bytes], source_context: Optional[str] = None) -> DocumentParseResult:
        start_time = datetime.now()
        raw_text = ""
        extracted_data: List[ExtractedField] = []
        status = "failed"
        error_message = None

        try:
            # Simulate OCR processing
            await asyncio.sleep(0.5)

            if isinstance(image_data, str) and image_data.startswith("http"):
                raw_text = "Simulated OCR from URL: Total: $123.45, Date: 2026-05-20, Vendor: ExampleCo"
            elif isinstance(image_data, str) and image_data.startswith("data:image/"):
                raw_text = "Simulated OCR from Base64: Total: $50.00, Date: 2026-05-19, Vendor: StoreXYZ"
            elif isinstance(image_data, bytes):
                raw_text = "Simulated OCR from bytes: Total: $75.20, Date: 2026-05-18, Vendor: Groceries"
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
            # Simulate Audio-to-Text processing
            await asyncio.sleep(0.7)

            if isinstance(audio_data, str) and audio_data.startswith("http"):
                transcription = "Simulated transcription from URL: Customer said amount is one hundred dollars to account number one zero one zero."
            elif isinstance(audio_data, bytes):
                transcription = "Simulated transcription from bytes: Record a payment of two hundred fifty to accounts payable."
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

    async def process_multimodal_input(self, multimodal_input: MultimodalInput, jwt_token: Optional[str] = None) -> Union[DocumentParseResult, AudioParseResult, Dict[str, Any]]:
        if multimodal_input.input_type in ["image_url", "base64_image"]:
            return await self.process_document_ocr(multimodal_input.data, multimodal_input.source_context)
        elif multimodal_input.input_type in ["audio_url", "base64_audio"]:
            return await self.process_audio_to_text(multimodal_input.data, multimodal_input.source_context)
        elif multimodal_input.input_type == "text":
            await asyncio.sleep(0.3)
            # For text, we can also try to map to JE
            extracted_data_from_text: List[ExtractedField] = []
            # Simple keyword extraction for illustration
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
