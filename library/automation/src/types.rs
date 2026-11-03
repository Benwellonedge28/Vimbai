//! Task and execution vocabularies, ported from automation-engine's enums.

use serde::{Deserialize, Serialize};

/// The work a scheduled task performs. A strict list: an unknown value is a
/// rejection, not a new task type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    // Accounting
    ReconcileAccounts,
    GenerateTrialBalance,
    PostJournalEntries,
    DepreciationCalculation,
    YearEndClose,
    MonthEndClose,
    // Finance
    BudgetVarianceCheck,
    CashflowForecastUpdate,
    FinancialRatioCalculation,
    ScenarioRefresh,
    // Integrations
    SyncPosData,
    BankFeedProcessing,
    InventoryReconciliation,
    CrmSync,
    // Fraud and compliance
    FraudScan,
    ComplianceCheck,
    AuditTrailExport,
    // Reporting
    GenerateFinancialStatements,
    DashboardRefresh,
    ScheduledReportGeneration,
    // Multimodal
    DocumentProcessingBatch,
    OcrQueueProcessing,
    // System
    DataBackup,
    CacheClear,
    SyncOfflineData,
}

impl TaskType {
    pub const ALL: [TaskType; 25] = [
        TaskType::ReconcileAccounts,
        TaskType::GenerateTrialBalance,
        TaskType::PostJournalEntries,
        TaskType::DepreciationCalculation,
        TaskType::YearEndClose,
        TaskType::MonthEndClose,
        TaskType::BudgetVarianceCheck,
        TaskType::CashflowForecastUpdate,
        TaskType::FinancialRatioCalculation,
        TaskType::ScenarioRefresh,
        TaskType::SyncPosData,
        TaskType::BankFeedProcessing,
        TaskType::InventoryReconciliation,
        TaskType::CrmSync,
        TaskType::FraudScan,
        TaskType::ComplianceCheck,
        TaskType::AuditTrailExport,
        TaskType::GenerateFinancialStatements,
        TaskType::DashboardRefresh,
        TaskType::ScheduledReportGeneration,
        TaskType::DocumentProcessingBatch,
        TaskType::OcrQueueProcessing,
        TaskType::DataBackup,
        TaskType::CacheClear,
        TaskType::SyncOfflineData,
    ];

    /// The wire value, e.g. `reconcile_accounts`.
    pub fn value(self) -> &'static str {
        match self {
            TaskType::ReconcileAccounts => "reconcile_accounts",
            TaskType::GenerateTrialBalance => "generate_trial_balance",
            TaskType::PostJournalEntries => "post_journal_entries",
            TaskType::DepreciationCalculation => "depreciation_calculation",
            TaskType::YearEndClose => "year_end_close",
            TaskType::MonthEndClose => "month_end_close",
            TaskType::BudgetVarianceCheck => "budget_variance_check",
            TaskType::CashflowForecastUpdate => "cashflow_forecast_update",
            TaskType::FinancialRatioCalculation => "financial_ratio_calculation",
            TaskType::ScenarioRefresh => "scenario_refresh",
            TaskType::SyncPosData => "sync_pos_data",
            TaskType::BankFeedProcessing => "bank_feed_processing",
            TaskType::InventoryReconciliation => "inventory_reconciliation",
            TaskType::CrmSync => "crm_sync",
            TaskType::FraudScan => "fraud_scan",
            TaskType::ComplianceCheck => "compliance_check",
            TaskType::AuditTrailExport => "audit_trail_export",
            TaskType::GenerateFinancialStatements => "generate_financial_statements",
            TaskType::DashboardRefresh => "dashboard_refresh",
            TaskType::ScheduledReportGeneration => "scheduled_report_generation",
            TaskType::DocumentProcessingBatch => "document_processing_batch",
            TaskType::OcrQueueProcessing => "ocr_queue_processing",
            TaskType::DataBackup => "data_backup",
            TaskType::CacheClear => "cache_clear",
            TaskType::SyncOfflineData => "sync_offline_data",
        }
    }

    /// The enum member name the service advertised, e.g. `RECONCILE_ACCOUNTS`.
    pub fn name(self) -> &'static str {
        match self {
            TaskType::ReconcileAccounts => "RECONCILE_ACCOUNTS",
            TaskType::GenerateTrialBalance => "GENERATE_TRIAL_BALANCE",
            TaskType::PostJournalEntries => "POST_JOURNAL_ENTRIES",
            TaskType::DepreciationCalculation => "DEPRECIATION_CALCULATION",
            TaskType::YearEndClose => "YEAR_END_CLOSE",
            TaskType::MonthEndClose => "MONTH_END_CLOSE",
            TaskType::BudgetVarianceCheck => "BUDGET_VARIANCE_CHECK",
            TaskType::CashflowForecastUpdate => "CASHFLOW_FORECAST_UPDATE",
            TaskType::FinancialRatioCalculation => "FINANCIAL_RATIO_CALCULATION",
            TaskType::ScenarioRefresh => "SCENARIO_REFRESH",
            TaskType::SyncPosData => "SYNC_POS_DATA",
            TaskType::BankFeedProcessing => "BANK_FEED_PROCESSING",
            TaskType::InventoryReconciliation => "INVENTORY_RECONCILIATION",
            TaskType::CrmSync => "CRM_SYNC",
            TaskType::FraudScan => "FRAUD_SCAN",
            TaskType::ComplianceCheck => "COMPLIANCE_CHECK",
            TaskType::AuditTrailExport => "AUDIT_TRAIL_EXPORT",
            TaskType::GenerateFinancialStatements => "GENERATE_FINANCIAL_STATEMENTS",
            TaskType::DashboardRefresh => "DASHBOARD_REFRESH",
            TaskType::ScheduledReportGeneration => "SCHEDULED_REPORT_GENERATION",
            TaskType::DocumentProcessingBatch => "DOCUMENT_PROCESSING_BATCH",
            TaskType::OcrQueueProcessing => "OCR_QUEUE_PROCESSING",
            TaskType::DataBackup => "DATA_BACKUP",
            TaskType::CacheClear => "CACHE_CLEAR",
            TaskType::SyncOfflineData => "SYNC_OFFLINE_DATA",
        }
    }

    /// Strict parse. The service answered 422 for `ledger_sweep`; here the
    /// caller gets `UnknownTaskType` and can choose the status code.
    pub fn parse(value: &str) -> Result<TaskType, String> {
        TaskType::ALL
            .iter()
            .copied()
            .find(|t| t.value() == value)
            .ok_or_else(|| format!("Unknown task type: {value}"))
    }

    /// The `[{name, value}]` list the service published on `/task-types`.
    pub fn catalogue() -> Vec<serde_json::Value> {
        TaskType::ALL
            .iter()
            .map(|t| serde_json::json!({"name": t.name(), "value": t.value()}))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Skipped,
}

impl TaskStatus {
    pub fn value(self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
            TaskStatus::Skipped => "skipped",
        }
    }
}
