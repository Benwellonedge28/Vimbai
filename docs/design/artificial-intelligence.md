# AI and intelligent automation
Status: proposed extension of multimodal capture and automation. No autonomous financial authority.

## Use cases
OCR/document extraction, classification suggestions, duplicate detection, reconciliation assistance, anomaly flags, natural-language queries and workflow drafting. Model outputs are untrusted proposals with confidence, provenance and evidence. Deterministic policy and ledger kernel decide financial validity.

## Architecture
Extend existing multimodal inbox and automation owners; model-provider interface supports local and remote inference subject to consent and deployment policy. Tool permissions scoped to principal and Book; default read-only. Explicit approval for consequential actions; never allow model direct database writes, secret access or permission escalation. Treat document instructions as data, not agent commands.

## Data
No customer financial data used for model training by default. Separate opt-in, purpose limitation, minimization, retention, deletion and contractual rights. Prefer synthetic/deidentified evaluation data; assess reidentification risk. Log model/version and evidence without exposing secrets.

## Acceptance
Extraction accuracy by document type/language, hallucination and prompt-injection tests, unauthorized access attempts, offline fallback, human correction and audit replay. Disable unsafe model/provider without disabling core bookkeeping.
