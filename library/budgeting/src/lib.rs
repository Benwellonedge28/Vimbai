#![forbid(unsafe_code)]
//! Budgeting math: preparation, monitoring, variance analysis and
//! forecasting.
//!
//! Ported from: budget-service (prepare, monitor, analyze,
//! variance-analysis, forecast, rolling-forecast),
//! master-budget-service, cash-budget-service,
//! capital-expenditure-budget-service, zero-based-budget-service and
//! flexible-budget-service.
//!
//! All functions are pure and storage-free; the Python services keep
//! their persistence (Neo4j, Book scoping) until the seam is wired.

pub mod budget;
pub mod capex;
pub mod cash;
pub mod flexible;
pub mod master;
pub mod zero_based;

pub use budget::{
    analyze_budget_variance, detailed_variance_analysis, generate_forecast, monitor_budget,
    prepare_budget, rolling_forecast, BudgetItem, BudgetVarianceRequest, ForecastRequest,
    ForecastResult, MonitorResult, PrepareResult, Projection, RollingForecastResult,
    VarianceItemAnalysis, VarianceSummary,
};
pub use capex::{prepare_capex_budget, CapexProject, CapexResult, DeferredProject, FundedProject};
pub use cash::{
    prepare_cash_budget, CashBudgetPeriod, CashBudgetRequest, CashBudgetResult, CashFlowItem,
};
pub use flexible::{
    flex_budget, flex_multi_level, flex_variance_analysis, flexed_cost_per_unit, prepare_flexed,
    FlexMultiLevel, FlexReason, FlexVariance, FlexedPrepared,
};
pub use master::{prepare_master_budget, MasterBudgetRequest, MasterBudgetResult};
pub use zero_based::{
    prepare_zero_based_budget, PrioritizedItem, ZeroBasedRequest, ZeroBasedResult,
};
