# FinAcc Multimodal Pipeline Service

This service is a crucial component of the FinAcc application, responsible for processing various input types (images, audio, text) to extract relevant financial data. It acts as a pipeline to transform unstructured or semi-structured data into structured formats suitable for integration with other FinAcc microservices like the Accounting Service.

## Features

-   **Document OCR Processing:** Extracts text and structured data from images (e.g., receipts, invoices).
-   **Audio-to-Text Transcription:** Transcribes spoken financial instructions or data.
-   **General Multimodal Input Handler:** A unified endpoint to route and process different input types.
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.

## Getting Started

To run this service along with Neo4j and other FinAcc services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The service requires the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located.

-   `JWT_SECRET`: A strong, random secret key for signing JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` used by the `identity-service` for token validation to work.**

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `multimodal-pipeline-service` and other FinAcc Docker images.
2.  Start a Neo4j container and all other FinAcc microservices.

The Multimodal Pipeline Service will be accessible at `http://localhost:8002`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints now require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has `multimodal.process.ocr`, `multimodal.process.audio`, or `multimodal.process.any` permissions, or `SUPER_ADMIN` role.

#### **Step 2: Make Authenticated Requests to Multimodal Pipeline Service**

Use the copied JWT token in the `Authorization` header for all requests to the Multimodal Pipeline Service.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

**Permissions:**
-   `multimodal.process.ocr`: Required for `/process-document-ocr`.
-   `multimodal.process.audio`: Required for `/process-audio-to-text`.
-   `multimodal.process.any`: Required for `/process-general-multimodal-input`.
-   `SUPER_ADMIN` role has `*.*` permissions, allowing all operations.

---

### **Endpoints**

#### Process Document for OCR (Image Upload)

**Endpoint:** `POST http://localhost:8002/process-document-ocr`
**Headers:** `Content-Type: multipart/form-data`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Form Data:**
-   `file`: The image file (e.g., JPEG, PNG of a receipt or invoice).
-   `source_context` (optional): "Receipt from coffee shop"

Example (using `curl`):
```bash
curl -X POST "http://localhost:8002/process-document-ocr" \
         -H "accept: application/json" \
         -H "Authorization: Bearer <YOUR_JWT_TOKEN_HERE>" \
         -F "file=@/path/to/your/receipt.jpg;type=image/jpeg" \
         -F "source_context=Coffee shop receipt"
```

#### Process Audio for Transcription (Audio Upload)

**Endpoint:** `POST http://localhost:8002/process-audio-to-text`
**Headers:** `Content-Type: multipart/form-data`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Form Data:**
-   `file`: The audio file (e.g., WAV, MP3).
-   `source_context` (optional): "Voice note for expense entry"

#### Process General Multimodal Input (JSON Payload)

**Endpoint:** `POST http://localhost:8002/process-multimodal-input`
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "input_type": "image_url",
  "data": "https://example.com/receipt.jpg",
  "source_context": "Online receipt image"
}
```
Or:
```json
{
  "input_type": "text",
  "data": "Please record $50 for office supplies in account 5020.",
  "source_context": "Manual text input"
}
```

---

### 5. Development

If developing locally (outside Docker Compose), ensure you have Python (3.10+) installed and your environment variables (`JWT_SECRET`) are correctly set.

```bash
# Navigate to multimodal-pipeline-service directory
cd multimodal-pipeline-service

# Install dependencies
pip install -r requirements.txt

# Run locally
uvicorn main:app --host 0.0.0.0 --port 8002
```

## Future Enhancements

-   Integration with actual cloud-based OCR and ASR APIs (e.g., Google Vision/Speech-to-Text).
-   Advanced Natural Language Understanding (NLU) for extracting more complex financial entities and intentions from text/audio.
-   Automated generation of Journal Entries from parsed multimodal data, interacting with the Accounting Service.
-   Contextual understanding and statefulness for conversational interfaces.
-   Support for more document types and layouts.
