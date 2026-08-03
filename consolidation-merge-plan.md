# Consolidation Cluster Merge Plan

## Target Service: `consolidation-service`
This service will consolidate the functionality of:
- `consolidation-service` (Port: 8347)
- `consolidated-financial-statements-service` (Port: 8139)
- `consolidation-reporting-service` (Port: 8348)

## Endpoints to implement:
1. `POST /consolidate` - Comprehensive consolidation of parent and subsidiaries (including NCI and goodwill)
2. `POST /eliminations` - Calculate intercompany eliminations
3. `POST /currency-translation` - Translate foreign subsidiary financials to reporting currency
4. `POST /validate` - Validate consolidation results

## Models:
- `SubsidiaryData`
- `IntercompanyTransaction`
- `ConsolidationRequest`
- `ConsolidationResponse`
- `IntercompanyEliminationRequest`
- `IntercompanyEliminationResponse`
- `CurrencyTranslationRequest`
- `CurrencyTranslationResponse`
- `ConsolidationValidationRequest`
- `ConsolidationValidationResponse`
