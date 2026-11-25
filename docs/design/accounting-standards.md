# Accounting standards and jurisdictions
Status: proposed extension of accounting standards catalogue and compliance domain.

## Design
Represent standards as versioned policy packs with jurisdiction, entity eligibility, effective dates, reporting basis, recognition/measurement rules, disclosure templates, source references and reviewer approval. Support multiple reporting bases through explicit adjustments or separate reporting Books where legally appropriate; never overwrite historic facts.

## Integration
Extend existing accounting-standards service and taxation/compliance owners; calculations remain pure and canonical. Policy resolution takes Book profile, reporting date and transaction type. Missing or conflicting rules yield actionable errors. Do not claim IFRS, IPSAS or local statutory compliance merely because a pack exists.

## Lifecycle
Draft pack → expert review → test vectors → signed release → authorized activation → supersession. Store policy version with decisions and reports. Migration previews impacts; historical reports remain reproducible.

## Acceptance
Effective-date boundaries, jurisdiction conflicts, comparative reports, missing policy refusal, disclosure completeness and expert-reviewed golden cases. No fixed country list or universal tax rate.
