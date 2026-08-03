# CVP Analysis Cluster Merge Plan

## Target Service: `cvp-analysis-service`
This service will consolidate the functionality of:
- `break-even-point-service` (Port: 8079)
- `break-even-analysis-service` (Port: 8078)
- `break-even-revenue-service` (Port: 8080)
- `break-even-output-service` (Port: 8081)
- `contribution-per-unit-service` (Port: 8082)
- `contribution-analysis-service` (Port: 8083)

## Endpoints to implement:
1. `POST /analyze` - Comprehensive CVP analysis (break-even point, revenue, output, margin of safety, target profit)
2. `POST /target-profit` - Calculate required sales (units and revenue) for a target profit
3. `POST /multi-product` - Calculate break-even and target profit for a multi-product mix
4. `POST /contribution` - Calculate contribution margin per unit and ratio

## Models:
- `CVPAnalysisRequest`
- `CVPAnalysisResponse`
- `TargetProfitRequest`
- `TargetProfitResponse`
- `MultiProductRequest`
- `MultiProductResponse`
- `ContributionRequest`
- `ContributionResponse`
