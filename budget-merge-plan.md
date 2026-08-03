# Budgeting & Variance Cluster Merge Plan

## Target Service: `budget-service`
This service will consolidate the functionality of:
- `budget-variance-service` (Port: 8277)
- `budget-variance-analysis-service` (Port: 8305)
- `budget-monitoring-service` (Port: 8304)
- `budgeting-service` (Port: 8302)
- `budget-forecasting-service` (Port: 8344)

## Endpoints to implement:
1. `POST /budget` (from `budget-forecasting-service` and `budgeting-service`) - Create/prepare budget
2. `POST /analyze` (from `budget-variance-service`, `budget-variance-analysis-service`, and `budget-monitoring-service`) - Analyze variance and monitor budget
3. `POST /forecast` (from `budget-forecasting-service`) - Generate forecast
4. `POST /rolling-forecast` (from `budget-forecasting-service`) - Generate rolling forecast

## Models:
- `BudgetItem`
- `BudgetRequest`
- `BudgetResponse`
- `VarianceAnalysisRequest`
- `VarianceAnalysisResponse`
- `ForecastRequest`
- `ForecastResponse`
- `RollingForecastRequest`
- `RollingForecastResponse`
