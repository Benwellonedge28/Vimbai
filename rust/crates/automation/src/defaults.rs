//! The seven tasks the engine shipped pre-loaded with.

use serde_json::json;

use crate::task::TaskDefinition;
use crate::types::TaskType;

fn task(
    id: &str,
    name: &str,
    description: &str,
    task_type: TaskType,
    schedule: &str,
    endpoint: &str,
    timeout_seconds: u64,
) -> TaskDefinition {
    TaskDefinition {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(description.to_string()),
        task_type,
        schedule: schedule.to_string(),
        enabled: true,
        service_endpoint: endpoint.to_string(),
        payload: json!({}),
        timeout_seconds,
        retry_count: 3,
        retry_delay_seconds: 60,
        notification_on_failure: true,
        run_immediately: false,
        max_concurrent: 1,
    }
}

pub fn default_tasks() -> Vec<TaskDefinition> {
    vec![
        task(
            "daily-reconciliation",
            "Daily Account Reconciliation",
            "Automated daily bank account reconciliation",
            TaskType::ReconcileAccounts,
            "0 2 * * *",
            "/api/reconcile",
            300,
        ),
        task(
            "trial-balance-gen",
            "Generate Trial Balance",
            "Generate daily trial balance report",
            TaskType::GenerateTrialBalance,
            "0 1 * * *",
            "/api/trial-balance/generate",
            300,
        ),
        task(
            "fraud-scan-daily",
            "Daily Fraud Scan",
            "Scan all transactions for fraud patterns",
            TaskType::FraudScan,
            "0 3 * * *",
            "/api/fraud/scan-all",
            300,
        ),
        task(
            "budget-variance-check",
            "Budget Variance Check",
            "Check budget variances and alert",
            TaskType::BudgetVarianceCheck,
            "0 9 * * *",
            "/api/budgets/check-variance",
            300,
        ),
        task(
            "cashflow-forecast",
            "Cash Flow Forecast Update",
            "Update cash flow forecast projections",
            TaskType::CashflowForecastUpdate,
            "0 6 * * *",
            "/api/finance/update-forecast",
            300,
        ),
        task(
            "dashboard-refresh",
            "Dashboard Refresh",
            "Refresh all dashboard data",
            TaskType::DashboardRefresh,
            "*/15 * * * *",
            "/api/dashboards/refresh",
            120,
        ),
        task(
            "month-end-close",
            "Month-End Close",
            "Execute month-end closing procedures",
            TaskType::MonthEndClose,
            "0 0 28-31 * *",
            "/api/accounting/month-end",
            300,
        ),
    ]
}
