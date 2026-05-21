from neo4j import AsyncSession
from multimodal_pipeline_service import models, crud
from datetime import datetime
from typing import Optional, List, Dict, Any
import asyncio
import random # For mocking confidence scores

class AIProcessor:
    def __init__(self, db_session: AsyncSession):
        self.db_session = db_session

    async def _mock_ocr_processing(self, input_url: str) -> models.DocumentParseResult:
        """Mocks OCR processing of a document."""
        await asyncio.sleep(2) # Simulate processing time
        return models.DocumentParseResult(
            raw_text="Sample OCR Output: Vendor: Starbucks, Date: 2026-05-20, Total: $5.45",
            extracted_data=[
                models.ExtractedDataField(name="vendor_name", value="Starbucks", confidence=0.95, data_type="string"),
                models.ExtractedDataField(name="date", value="2026-05-20", confidence=0.92, data_type="date"),
                models.ExtractedDataField(name="total_amount", value="5.45", confidence=0.88, data_type="currency", unit="USD"),
            ],
            ai_confidence=0.91
        )

    async def _mock_asr_processing(self, input_url: str) -> models.AudioParseResult:
        """Mocks ASR (Speech-to-Text) processing of audio."""
        await asyncio.sleep(3) # Simulate processing time
        return models.AudioParseResult(
            transcribed_text="Add a new expense for taxi fare, fifty dollars, category travel.",
            extracted_commands=[
                models.ExtractedDataField(name="action", value="create_expense", confidence=0.98, data_type="string"),
                models.ExtractedDataField(name="description", value="taxi fare", confidence=0.94, data_type="string"),
                models.ExtractedDataField(name="amount", value="50.00", confidence=0.96, data_type="number"),
                models.ExtractedDataField(name="category", value="travel", confidence=0.90, data_type="string"),
            ],
            ai_confidence=0.95
        )

    async def process_multimodal_task(self, task_id: str):
        """Main entry point for processing a multimodal task in the background."""
        task = await crud.get_multimodal_processing_task(self.db_session, task_id)
        if not task:
            print(f"AIProcessor: Task {task_id} not found.")
            return

        # Update status to processing
        await crud.update_multimodal_processing_task(self.db_session, task_id, models.MultimodalProcessingTaskUpdate(
            status="processing",
            processing_start_time=datetime.utcnow()
        ))

        try:
            result_update = models.MultimodalProcessingTaskUpdate()
            if task.input_type == "document" or task.input_type == "image":
                # For document/image, perform OCR/extraction
                doc_result = await self._mock_ocr_processing(task.input_url or "")
                result_update.document_result = doc_result
                # Logic to suggest a journal entry based on extracted data
                result_update.suggested_journal_entry = {
                    "description": next((f.value for f in doc_result.extracted_data if f.name == "vendor_name"), "Manual Entry"),
                    "amount": next((f.value for f in doc_result.extracted_data if f.name == "total_amount"), "0.00"),
                    "date": next((f.value for f in doc_result.extracted_data if f.name == "date"), datetime.now().date().isoformat())
                }
            elif task.input_type == "audio":
                # For audio, perform ASR
                audio_result = await self._mock_asr_processing(task.input_url or "")
                result_update.audio_result = audio_result
                # Suggest JE based on transcribed commands
                result_update.suggested_journal_entry = {
                    "description": next((f.value for f in audio_result.extracted_commands if f.name == "description"), "Voice Entry"),
                    "amount": next((f.value for f in audio_result.extracted_commands if f.name == "amount"), "0.00"),
                }
            
            # Finalize task status based on confidence
            # If confidence is low, set to review_pending
            overall_confidence = (result_update.document_result.ai_confidence if result_update.document_result else 
                                  result_update.audio_result.ai_confidence if result_update.audio_result else 0.0)
            
            result_update.status = "ai_extracted" if overall_confidence > 0.9 else "review_pending"
            result_update.processing_end_time = datetime.utcnow()
            
            await crud.update_multimodal_processing_task(self.db_session, task_id, result_update)
            print(f"AIProcessor: Task {task_id} processing completed with status {result_update.status}.")

        except Exception as e:
            print(f"AIProcessor Error: Failed to process task {task_id}: {e}")
            await crud.update_multimodal_processing_task(self.db_session, task_id, models.MultimodalProcessingTaskUpdate(
                status="failed",
                errors=[str(e)],
                processing_end_time=datetime.utcnow()
            ))
