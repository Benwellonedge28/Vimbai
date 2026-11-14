//! The record store every service crate reads and writes through.
//!
//! The calculation crates in `crates/calculations` compute and never touch a database. The service
//! crates in `crates/domains` hold records, and they all do it through this one crate rather than
//! each carrying its own copy of the connection and the scoping rules. This crate is the machinery
//! only: the record shapes, the Book scoping and the Neo4j driver. No service's arithmetic lives
//! here. The Python layer wrote the same shape, so existing data stays readable:
//!
//! ```text
//! (u:User {id: $user_id})-[:OWNS_X]->(x:Label {..., user_id, book_id})
//! ```
//!
//! Two rules are enforced here rather than left to each caller:
//!
//! * every write is stamped with the caller's `user_id` and `book_id`, and a payload cannot
//!   overwrite either, so one Book can never write into another;
//! * every read is filtered by the same pair, so a record belonging to another Book is a miss
//!   rather than a leak.
//!
//! Values travel as query parameters, never as formatted Cypher. That is what keeps a product
//! name like `Menswear (Q3)` from breaking the statement or injecting one.

#![forbid(unsafe_code)]

use serde_json::{json, Map, Value};

pub mod amounts;
/// A graph that records what it was asked and answers from a script, for the tests of the crates
/// that sit on this one.
#[doc(hidden)]
pub mod fake;
pub mod file;
pub mod neo4j_client;
pub mod query;
pub mod read;
pub mod shared;
pub mod write;

pub use neo4j_client::Neo4jGraph;

/// Who is asking, and which Book they are asking about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookScope {
    pub user_id: String,
    pub book_id: String,
}

impl BookScope {
    pub fn new(user_id: impl Into<String>, book_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            book_id: book_id.into(),
        }
    }

    fn params(&self) -> Value {
        json!({"user_id": self.user_id, "book_id": self.book_id})
    }
}

/// A stored record type: the node label and the edge the owning user reaches it by.
/// The label a record is written under, and the edge that hangs it off its owner.
///
/// Every query in [`Records`] matches on the label *and* the edge, so a listing answers one kind and one
/// kind only. That is what lets an edge be shared where two kinds genuinely belong together - the equity
/// statement and the cash flow statement both hang off `OWNS_STATEMENT`, an equity, intercompany and fund
/// transaction off `OWNS_TRANSACTION` - without either service seeing the other's rows. Renaming those
/// edges is not a cleanup: the query would stop matching the nodes already written, so the records would
/// go quietly missing. A shared edge is safe; a shared label would not be, and the label table is checked
/// for that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordKind {
    pub label: &'static str,
    pub edge: &'static str,
}

impl RecordKind {
    pub const fn new(label: &'static str, edge: &'static str) -> Self {
        Self { label, edge }
    }
}

/// The records absorption-costing-service used to own.
pub const PRODUCT_COST: RecordKind = RecordKind::new("ProductCost", "OWNS_PRODUCT_COST");
pub const OVERHEAD_ABSORPTION: RecordKind =
    RecordKind::new("OverheadAbsorption", "OWNS_OVERHEAD_ABSORPTION");

/// The records job-costing-service used to own.
pub const JOB: RecordKind = RecordKind::new("Job", "OWNS_JOB");
pub const JOB_COST_ENTRY: RecordKind = RecordKind::new("JobCostEntry", "HAS_COST");

/// The records insurance-claims-service used to own.
pub const INSURANCE_CLAIM: RecordKind = RecordKind::new("InsuranceClaim", "OWNS_CLAIM");

/// The records treasury-analytics-service used to own.
/// The records cash-flow-statement-service used to own.
/// The records equity-changes-service used to own.
/// The records cash-optimization-service used to own.
/// The records trading-account-service used to own.
/// The records appropriation-control-service used to own.
pub const APPROPRIATION: RecordKind = RecordKind::new("Appropriation", "OWNS_APPROPRIATION");
pub const APPROPRIATION_TX: RecordKind =
    RecordKind::new("AppropriationTransaction", "OWNS_APPROPRIATION_TX");

pub const TRADING_ACCOUNT: RecordKind = RecordKind::new("TradingAccount", "OWNS_TRADING_ACCOUNT");

pub const CASH_ACCOUNT: RecordKind = RecordKind::new("CashAccount", "OWNS_ACCOUNT");
pub const CASH_SUGGESTION: RecordKind =
    RecordKind::new("OptimizationSuggestion", "OWNS_SUGGESTION");

pub const TAX_RATE: RecordKind = RecordKind::new("TaxRate", "OWNS_TAX_RATE");
pub const TAX_REGISTRATION: RecordKind =
    RecordKind::new("TaxRegistration", "OWNS_TAX_REGISTRATION");
pub const TAX_TRANSACTION: RecordKind = RecordKind::new("TaxTransaction", "OWNS_TAX_TRANSACTION");
pub const TAX_RETURN: RecordKind = RecordKind::new("TaxReturn", "OWNS_TAX_RETURN");
pub const WITHHOLDING_ENTRY: RecordKind = RecordKind::new("WithholdingEntry", "OWNS_WITHHOLDING");
pub const TAX_REPORT: RecordKind = RecordKind::new("TaxReport", "OWNS_TAX_REPORT");
pub const DEFERRED_TAX: RecordKind = RecordKind::new("DeferredTax", "OWNS_DEFERRED_TAX");
pub const TAX_OBLIGATION: RecordKind = RecordKind::new("TaxObligation", "OWNS_OBLIGATION");
pub const TAX_STRATEGY: RecordKind = RecordKind::new("TaxStrategy", "OWNS_STRATEGY");
pub const EQUITY_TRANSACTION: RecordKind = RecordKind::new("EquityTransaction", "OWNS_TRANSACTION");
pub const EQUITY_STATEMENT: RecordKind = RecordKind::new("EquityStatement", "OWNS_STATEMENT");

pub const CASH_FLOW_STATEMENT: RecordKind = RecordKind::new("CashFlowStatement", "OWNS_STATEMENT");

pub const TREASURY_SNAPSHOT: RecordKind =
    RecordKind::new("TreasuryAnalyticsSnapshot", "OWNS_SNAPSHOT");

/// The records process-costing-service and product-costing-service both own.
///
/// The two services are the same thing twice: same models, same endpoints, same label, and a list
/// query that filters by company only, so each one already answers with the other's records. One
/// constant serves both, because splitting the label would invent a distinction the data never made.
pub const COST_CALCULATION: RecordKind = RecordKind::new("CostCalculation", "OWNS_CALCULATION");

/// The apportionment rules basis-apportionment-service owns.
pub const APPORTIONMENT_RULE: RecordKind =
    RecordKind::new("ApportionmentRule", "OWNS_APPORTIONMENT_RULE");

/// The apportionments that service has made.
pub const APPORTIONMENT_RESULT: RecordKind =
    RecordKind::new("ApportionmentResult", "OWNS_APPORTIONMENT_RESULT");

/// The absorption rates overhead-absorption-rate-service owns.
pub const ABSORPTION_RATE: RecordKind =
    RecordKind::new("OverheadAbsorptionRate", "OWNS_ABSORPTION_RATE");

/// The rate calculations that same service records beside them.
pub const OAR_ANALYSIS: RecordKind = RecordKind::new("OARAnalysis", "OWNS_OAR_ANALYSIS");

/// The cost-volume-profit analyses cvp-analysis-service owns.
pub const CVP_ANALYSIS: RecordKind = RecordKind::new("CVPAnalysis", "OWNS_CVP_ANALYSIS");

/// The fixed asset register fixed-assets-register-service owns.
pub const FIXED_ASSET: RecordKind = RecordKind::new("FixedAsset", "OWNS_FIXED_ASSET");
pub const DEPRECIATION_ENTRY: RecordKind =
    RecordKind::new("DepreciationEntry", "OWNS_DEPRECIATION_ENTRY");
pub const ASSET_DISPOSAL: RecordKind = RecordKind::new("AssetDisposal", "OWNS_ASSET_DISPOSAL");
/// The intangibles intangible-assets-service owns.
pub const INTANGIBLE_ASSET: RecordKind =
    RecordKind::new("IntangibleAsset", "OWNS_INTANGIBLE_ASSET");
pub const AMORTIZATION_ENTRY: RecordKind =
    RecordKind::new("AmortizationEntry", "OWNS_AMORTIZATION_ENTRY");

/// The records disposal-group-service owns.
pub const DISPOSAL_GROUP: RecordKind = RecordKind::new("DisposalGroup", "OWNS_DISPOSAL_GROUP");
pub const DISPOSAL_ASSET: RecordKind = RecordKind::new("DisposalAsset", "OWNS_DISPOSAL_ASSET");
pub const IMPAIRMENT_TEST: RecordKind = RecordKind::new("ImpairmentTest", "OWNS_IMPAIRMENT_TEST");

/// The records lease-termination-service owns.
pub const LEASE_TERMINATION: RecordKind =
    RecordKind::new("LeaseTermination", "OWNS_LEASE_TERMINATION");
pub const TERMINATION_SETTLEMENT: RecordKind =
    RecordKind::new("TerminationSettlement", "OWNS_TERMINATION_SETTLEMENT");

/// debt-management-service's records: the loans a company carries.
pub const DEBT_LOAN: RecordKind = RecordKind::new("DebtLoan", "OWNS_DEBT_LOAN");

/// treasury-compliance-service's records: the regulatory checks a treasury runs, and their verdicts.
///
/// Deliberately NOT merged with `COMPLIANCE_CHECK`, which shares the `ComplianceCheck` name: that one records
/// whether a Book's accounting follows its chosen standard, this one records treasury regulation checks. One
/// label would let a standards verdict land in a treasury compliance rate.
pub const TREASURY_COMPLIANCE_CHECK: RecordKind =
    RecordKind::new("TreasuryComplianceCheck", "OWNS_TREASURY_COMPLIANCE_CHECK");

/// sovereign-treasury-service's records: a country's accounts, its debt, and one fiscal position a year.
pub const SOVEREIGN_ACCOUNT: RecordKind =
    RecordKind::new("SovereignAccount", "OWNS_SOVEREIGN_ACCOUNT");
pub const SOVEREIGN_DEBT: RecordKind = RecordKind::new("SovereignDebt", "OWNS_SOVEREIGN_DEBT");
pub const FISCAL_POSITION: RecordKind = RecordKind::new("FiscalPosition", "OWNS_FISCAL_POSITION");

/// treasury-reporting-service's records: the reports it generated and the figures they carry.
pub const TREASURY_REPORT: RecordKind = RecordKind::new("TreasuryReport", "OWNS_TREASURY_REPORT");

/// treasury-risk-service's records: the exposures a Book carries, the risk it measures on them, and the
/// stress scenarios it runs.
pub const RISK_EXPOSURE: RecordKind = RecordKind::new("RiskExposure", "OWNS_RISK_EXPOSURE");
pub const VAR_RESULT: RecordKind = RecordKind::new("VaRResult", "OWNS_VAR_RESULT");
pub const STRESS_SCENARIO: RecordKind = RecordKind::new("StressScenario", "OWNS_STRESS_SCENARIO");
pub const STRESS_RESULT: RecordKind = RecordKind::new("StressResult", "OWNS_STRESS_RESULT");

/// treasury-policy-service's records: the policies themselves, the limits each sets, and the checks made
/// against those limits.
pub const TREASURY_POLICY: RecordKind = RecordKind::new("TreasuryPolicy", "OWNS_TREASURY_POLICY");
pub const POLICY_LIMIT: RecordKind = RecordKind::new("PolicyLimit", "OWNS_POLICY_LIMIT");
pub const POLICY_CHECK: RecordKind = RecordKind::new("PolicyCheck", "OWNS_POLICY_CHECK");

/// treasury-management-service's records: a company's daily cash movements, and the position it reports.
///
/// Deliberately NOT merged with `CASH_FLOW_ENTRY`, which shares the `CashFlowEntry` name: that record is a
/// project appraisal year (inflow, outflow, net) and this one is a dated movement with a direction, so one
/// label would make each other's rows meaningless to the other's arithmetic.
pub const TREASURY_CASH_FLOW: RecordKind =
    RecordKind::new("TreasuryCashFlow", "OWNS_TREASURY_CASHFLOW");
pub const TREASURY_CASH_POSITION: RecordKind =
    RecordKind::new("TreasuryCashPosition", "OWNS_TREASURY_POSITION");

/// The records cash-management-service owns: money moving between its own accounts, and the liquidity it
/// measures. Its cash accounts are `CASH_ACCOUNT` above, which cash-optimization-service also uses: both
/// wrote the same `:CashAccount` label under different edges, so each could only ever see its own.
pub const CASH_TRANSFER: RecordKind = RecordKind::new("CashTransfer", "OWNS_CASH_TRANSFER");
pub const LIQUIDITY_POSITION: RecordKind =
    RecordKind::new("LiquidityPosition", "OWNS_LIQUIDITY_POSITION");

/// accounting-service's supporting records: petty cash, reconciliation, and the single-entry statements.
pub const PETTY_CASH_FUND: RecordKind = RecordKind::new("PettyCashFund", "OWNS_PETTY_CASH_FUND");
/// rolling-forecast-service's forecasts, with their periods kept on the forecast.
pub const ROLLING_FORECAST: RecordKind = RecordKind::new("RollingForecast", "OWNS_FORECAST");

/// scenario-analysis-service's compared scenarios, with their assumptions.
pub const SCENARIO: RecordKind = RecordKind::new("Scenario", "OWNS_SCENARIO");

/// sensitivity-analysis-service's completed analyses.
pub const SENSITIVITY_ANALYSIS: RecordKind =
    RecordKind::new("SensitivityAnalysis", "OWNS_ANALYSIS");

/// scenario-modeling-service's modelled scenarios, with their variables and the rules they call on.
pub const SCENARIO_MODEL: RecordKind = RecordKind::new("ScenarioModel", "OWNS_SCENARIO_MODEL");

/// scenario-modeling-service's rules: conditions to meet and what to do when they are met.
pub const MODELING_RULE: RecordKind = RecordKind::new("ModelingRule", "OWNS_MODELING_RULE");

/// one what-if run against a modelled scenario.
pub const WHAT_IF_RESULT: RecordKind = RecordKind::new("WhatIfModelResult", "OWNS_WHATIF_RESULT");

/// one swept variable against a modelled scenario.
pub const SENSITIVITY_MODEL_RESULT: RecordKind =
    RecordKind::new("SensitivityModelResult", "OWNS_SENSITIVITY_RESULT");

/// zero-based-budgeting-service's packages, with their items kept on the package.
pub const ZERO_BASED_PACKAGE: RecordKind = RecordKind::new("ZBBPackage", "OWNS_ZBB_PACKAGE");

/// personal-finance-service: the household's recurring items, debts, payments, investments and trades.
pub const PERSONAL_RECURRING: RecordKind = RecordKind::new("PersonalRecurring", "OWNS_RECURRING");
pub const PERSONAL_DEBT: RecordKind = RecordKind::new("PersonalDebt", "OWNS_PERSONAL_DEBT");
pub const PERSONAL_PAYMENT: RecordKind =
    RecordKind::new("PersonalDebtPayment", "OWNS_DEBT_PAYMENT");
pub const PERSONAL_INVESTMENT: RecordKind =
    RecordKind::new("PersonalInvestment", "OWNS_PERSONAL_INVESTMENT");
pub const PERSONAL_TRADE: RecordKind = RecordKind::new("PersonalTrade", "OWNS_PERSONAL_TRADE");
pub const PERSONAL_TAX_BRACKET: RecordKind = RecordKind::new("TaxBracket", "OWNS_TAX_BRACKET");
pub const FINANCIAL_GOAL: RecordKind = RecordKind::new("FinancialGoal", "OWNS_FINANCIAL_GOAL");
pub const INCOME_SOURCE: RecordKind = RecordKind::new("IncomeSource", "OWNS_INCOME_SOURCE");
pub const LEGACY_DEBT_ITEM: RecordKind = RecordKind::new("LegacyDebtItem", "OWNS_LEGACY_DEBT");

/// investment-monitoring-service's portfolios, what they hold, and how each was measured.
pub const INVESTMENT_PORTFOLIO: RecordKind =
    RecordKind::new("InvestmentPortfolio", "OWNS_INVESTMENT_PORTFOLIO");
pub const INVESTMENT_HOLDING: RecordKind =
    RecordKind::new("InvestmentHolding", "OWNS_INVESTMENT_HOLDING");
pub const PERFORMANCE_METRIC: RecordKind =
    RecordKind::new("PerformanceMetric", "OWNS_PERFORMANCE_METRIC");

/// trade-finance-service's instruments: letters of credit, documentary collections, bank guarantees.
pub const TRADE_INSTRUMENT: RecordKind =
    RecordKind::new("TradeInstrument", "OWNS_TRADE_INSTRUMENT");

/// continue-shutdown-decision-service's decisions: whether a department, product or division carries on trading.
pub const CONTINUE_SHUTDOWN_DECISION: RecordKind = RecordKind::new(
    "ContinueShutdownDecision",
    "OWNS_CONTINUE_SHUTDOWN_DECISION",
);

pub const PETTY_CASH_ENTRY: RecordKind = RecordKind::new("PettyCashEntry", "OWNS_PETTY_CASH_ENTRY");

/// petty-cash-service's own records. Its funds are `PETTY_CASH_FUND` above, which accounting-service's petty
/// cash book also writes: one label under one edge, so the funds both describe are one set. What is left is
/// what only this service keeps - the spending, the vouchers that authorise it and the replenishments.
pub const PETTY_CASH_TRANSACTION: RecordKind =
    RecordKind::new("PettyCashTransaction", "OWNS_PETTY_CASH_TRANSACTION");
pub const PETTY_CASH_VOUCHER: RecordKind =
    RecordKind::new("PettyCashVoucher", "OWNS_PETTY_CASH_VOUCHER");
pub const PETTY_CASH_REPLENISHMENT: RecordKind =
    RecordKind::new("PettyCashReplenishment", "OWNS_PETTY_CASH_REPLENISHMENT");
pub const BANK_RECONCILIATION: RecordKind =
    RecordKind::new("BankReconciliation", "OWNS_BANK_RECONCILIATION");
// bank-reconciliation-service keeps its statements, its cash book entries and its reconciliations under
// their own names. The shipped service wrote its reconciliations to `BankReconciliation` / `OWNS_BANK_RECONCILIATION`,
// which cashbook-service already uses for a different shape - one label, two record shapes, so a read of one
// service could see the other's rows. The split keeps each service's records to itself.
pub const BANK_STATEMENT: RecordKind = RecordKind::new("BankStatement", "OWNS_BANK_STATEMENT");
pub const BANK_REC_CASH_ENTRY: RecordKind =
    RecordKind::new("BankRecCashEntry", "OWNS_BANKREC_CASH_ENTRY");
pub const BANK_REC_RECONCILIATION: RecordKind =
    RecordKind::new("BankRecReconciliation", "OWNS_BANKREC_RECONCILIATION");
// The bank a Book deals with, under one label and one edge. bank-feed-service and banking-integration-service
// both keep connections, and each wrote its own label - `BankConnection` under two different edges, so the two
// could not see each other's rows. A connection is a connection: one label, one edge, and both services read
// the same set. A row keeps the properties its own service stated; the other service answers the fields it
// needs with defaults.
pub const BANK_CONNECTION: RecordKind = RecordKind::new("BankConnection", "OWNS_CONNECTION");
// The transactions a bank feed imports, likewise one set: bank-feed-service wrote `BankFeedTransaction` and
// banking-integration-service wrote `BankTransaction` for the same thing - money that arrived from the bank.
pub const BANK_FEED_TRANSACTION: RecordKind =
    RecordKind::new("BankFeedTransaction", "OWNS_BANK_TX");
pub const BANK_FEED_RULE: RecordKind = RecordKind::new("BankReconciliationRule", "OWNS_BANK_RULE");
pub const BANK_SYNC: RecordKind = RecordKind::new("BankSync", "OWNS_BANK_SYNC");
// bank-relationship-service's own records: the bank's own people and the service the Book gets from them.
pub const BANK_RELATIONSHIP: RecordKind = RecordKind::new("BankRelationship", "OWNS_RELATIONSHIP");
pub const BANK_QUALITY_METRIC: RecordKind =
    RecordKind::new("ServiceQualityMetric", "OWNS_QUALITY_METRIC");

// currency-service's own records: the currencies a Book deals in and the rates between them, each with the
// date it took effect, because a rate is a series rather than a single value.
pub const CURRENCY: RecordKind = RecordKind::new("Currency", "OWNS_CURRENCY");
pub const EXCHANGE_RATE: RecordKind = RecordKind::new("ExchangeRate", "OWNS_RATE");
pub const STATEMENT_OF_AFFAIRS: RecordKind =
    RecordKind::new("StatementOfAffairs", "OWNS_STATEMENT_OF_AFFAIRS");
pub const CAPITAL_CALCULATION: RecordKind =
    RecordKind::new("CapitalCalculation", "OWNS_CAPITAL_CALCULATION");
// compliance: the SOX controls a Book tests and the risks it carries. sox-compliance-service keeps controls,
// their tests and the deficiencies they raise; risk-assessment-service keeps the risk register. A control and a
// risk are different records, so each keeps its own label rather than sharing one shape.
pub const SOX_CONTROL: RecordKind = RecordKind::new("SoxControl", "OWNS_CONTROL");
pub const SOX_CONTROL_TEST: RecordKind = RecordKind::new("SoxControlTest", "OWNS_CONTROL_TEST");
pub const SOX_DEFICIENCY: RecordKind = RecordKind::new("SoxDeficiency", "OWNS_DEFICIENCY");
pub const RISK_ITEM: RecordKind = RecordKind::new("RiskItem", "OWNS_RISK");
// audit: the engagements an auditor runs and the evidence it draws. An engagement carries its findings as a
// property rather than as separate nodes, as shipped - the finding is only ever read back with its engagement.
pub const AUDIT_ENGAGEMENT: RecordKind = RecordKind::new("AuditEngagement", "OWNS_ENGAGEMENT");
pub const CONTROL_ACCOUNT: RecordKind = RecordKind::new("ControlAccount", "OWNS_CONTROL_ACCOUNT");
pub const RECEIPTS_PAYMENTS: RecordKind =
    RecordKind::new("ReceiptsPaymentsAccount", "OWNS_RECEIPTS_PAYMENTS");
pub const SINGLE_ENTRY_CONVERSION: RecordKind =
    RecordKind::new("SingleEntryConversion", "OWNS_SINGLE_ENTRY_CONVERSION");
pub const PROFIT_ESTIMATION: RecordKind =
    RecordKind::new("ProfitEstimation", "OWNS_PROFIT_ESTIMATION");

/// The subsidiary journals accounting-service owns, and the vendor bill beside them.
pub const SALES_JOURNAL: RecordKind =
    RecordKind::new("SalesJournalEntry", "OWNS_SALES_JOURNAL_ENTRY");
pub const PURCHASES_JOURNAL: RecordKind =
    RecordKind::new("PurchasesJournalEntry", "OWNS_PURCHASES_JOURNAL_ENTRY");
pub const CASH_RECEIPTS_JOURNAL: RecordKind =
    RecordKind::new("CashReceiptsJournalEntry", "OWNS_CASH_RECEIPT");
pub const CASH_DISBURSEMENTS_JOURNAL: RecordKind =
    RecordKind::new("CashDisbursementsJournalEntry", "OWNS_CASH_DISBURSEMENT");
pub const SALES_RETURNS_JOURNAL: RecordKind =
    RecordKind::new("SalesReturnsJournalEntry", "OWNS_SALES_RETURN");
pub const PURCHASES_RETURNS_JOURNAL: RecordKind =
    RecordKind::new("PurchasesReturnsJournalEntry", "OWNS_PURCHASES_RETURN");
pub const VENDOR_BILL: RecordKind = RecordKind::new("VendorBill", "OWNS_VENDOR_BILL");

/// The records accounting-service owns: the chart of accounts and the journal written against it.
pub const LEDGER_ACCOUNT: RecordKind = RecordKind::new("Account", "OWNS_LEDGER_ACCOUNT");
pub const JOURNAL_ENTRY: RecordKind = RecordKind::new("JournalEntry", "OWNS_JOURNAL_ENTRY");

/// The records accounting-standards-service owns.
pub const STANDARDS_CONFIG: RecordKind =
    RecordKind::new("StandardsConfig", "OWNS_STANDARDS_CONFIG");
pub const ACCOUNT_MAPPING: RecordKind = RecordKind::new("AccountMapping", "OWNS_ACCOUNT_MAPPING");
pub const ACCOUNTING_POLICY: RecordKind =
    RecordKind::new("AccountingPolicy", "OWNS_ACCOUNTING_POLICY");
pub const COMPLIANCE_CHECK: RecordKind =
    RecordKind::new("ComplianceCheck", "OWNS_COMPLIANCE_CHECK");

/// The records departmental-accounting-service owns.
pub const DEPARTMENT: RecordKind = RecordKind::new("Department", "OWNS_DEPARTMENT");
pub const DEPARTMENT_ALLOCATION_RULE: RecordKind =
    RecordKind::new("DepartmentAllocationRule", "OWNS_DEPT_ALLOCATION_RULE");
pub const DEPARTMENT_COST_POOL: RecordKind =
    RecordKind::new("DepartmentCostPool", "OWNS_DEPT_COST_POOL");
pub const DEPARTMENT_ALLOCATION: RecordKind =
    RecordKind::new("DepartmentAllocation", "OWNS_DEPT_ALLOCATION");
pub const INTER_DEPARTMENT_BILL: RecordKind =
    RecordKind::new("InterDepartmentBill", "OWNS_INTER_DEPT_BILL");
pub const DEPARTMENT_FINANCIALS: RecordKind =
    RecordKind::new("DepartmentFinancials", "OWNS_DEPT_FINANCIALS");

/// The records company-accounting-service owns.
pub const COMPANY: RecordKind = RecordKind::new("Company", "OWNS_COMPANY");
pub const COMPANY_SHARE_CAPITAL: RecordKind =
    RecordKind::new("CompanyShareCapital", "OWNS_COMPANY_SHARE_CAPITAL");
pub const CAPITAL_TRANSACTION: RecordKind =
    RecordKind::new("CapitalTransaction", "OWNS_CAPITAL_TRANSACTION");
pub const DIVIDEND: RecordKind = RecordKind::new("Dividend", "OWNS_DIVIDEND");
pub const DIVIDEND_PAYMENT: RecordKind =
    RecordKind::new("DividendPayment", "OWNS_DIVIDEND_PAYMENT");
pub const RETAINED_EARNINGS: RecordKind =
    RecordKind::new("RetainedEarnings", "OWNS_RETAINED_EARNINGS");
pub const COMPANY_RESERVE: RecordKind = RecordKind::new("CompanyReserve", "OWNS_COMPANY_RESERVE");

/// The records cashbook-service owns.
pub const CASH_BANK_ACCOUNT: RecordKind = RecordKind::new("CashBankAccount", "OWNS_CASH_ACCOUNT");
pub const CASH_ENTRY: RecordKind = RecordKind::new("CashBookEntry", "OWNS_CASH_ENTRY");
pub const CASH_RECONCILIATION: RecordKind =
    RecordKind::new("CashBookReconciliation", "OWNS_CASH_RECON");
pub const CASH_FLOW_ENTRY: RecordKind = RecordKind::new("CashFlowEntry", "OWNS_CASH_FLOW");

/// The records ifrs-reporting-service owns.
pub const IFRS_REPORT: RecordKind = RecordKind::new("IfrsReport", "OWNS_IFRS_REPORT");
pub const DISCLOSURE_NOTE: RecordKind = RecordKind::new("DisclosureNote", "OWNS_DISCLOSURE_NOTE");

/// The records intercompany-service owns.
pub const INTERCOMPANY_ENTITY: RecordKind = RecordKind::new("IntercompanyEntity", "OWNS_ENTITY");
pub const INTERCOMPANY_TRANSACTION: RecordKind =
    RecordKind::new("IntercompanyTransaction", "OWNS_TRANSACTION");
pub const INTERCOMPANY_ELIMINATION: RecordKind =
    RecordKind::new("EliminationEntry", "OWNS_ELIMINATION");

/// The records cash-flow-service owns.
pub const CASH_FLOW_SUMMARY: RecordKind = RecordKind::new("CashFlowSummary", "OWNS_CASH_FLOW");

/// The records balance-sheet-service owns.
pub const BALANCE_SHEET: RecordKind = RecordKind::new("BalanceSheet", "OWNS_SHEET");

/// The records fund-accounting-service owns.
pub const FUND: RecordKind = RecordKind::new("Fund", "OWNS_FUND");
pub const FUND_TRANSACTION: RecordKind = RecordKind::new("FundTransaction", "OWNS_TRANSACTION");

/// The records revenue-recognition-service owns.
pub const REVENUE_CONTRACT: RecordKind = RecordKind::new("RevenueContract", "OWNS_CONTRACT");

/// The records capital-reconstruction-service owns.
pub const CAPITAL_RECONSTRUCTION: RecordKind =
    RecordKind::new("CapitalReconstruction", "OWNS_RECONSTRUCTION");
pub const RECONSTRUCTION_ADJUSTMENT: RecordKind =
    RecordKind::new("ReconstructionAdjustment", "OWNS_ADJUSTMENT");
pub const RESERVE_CONVERSION: RecordKind = RecordKind::new("ReserveConversion", "OWNS_CONVERSION");

/// The records debentures-service owns.
pub const DEBENTURE_CLASS: RecordKind = RecordKind::new("DebentureClass", "OWNS_DEBENTURE_CLASS");
pub const DEBENTURE_ISSUE: RecordKind = RecordKind::new("DebentureIssue", "OWNS_ISSUE");
pub const DEBENTURE_INTEREST: RecordKind = RecordKind::new("InterestPayment", "OWNS_INTEREST");
pub const DEBENTURE_REDEMPTION: RecordKind = RecordKind::new("RedemptionEntry", "OWNS_REDEMPTION");

/// The records issued-share-capital-service owns.
pub const SHARE_ISSUE: RecordKind = RecordKind::new("ShareIssueNode", "OWNS_SHARE_ISSUE");
pub const SHAREHOLDER: RecordKind = RecordKind::new("ShareholderNode", "OWNS_SHAREHOLDER");

/// The records initial-investment-service owns.
pub const INITIAL_INVESTMENT: RecordKind =
    RecordKind::new("InitialInvestment", "OWNS_INITIAL_INVESTMENT");

/// The records share-redemption-service owns.
pub const SHARE_REDEMPTION: RecordKind =
    RecordKind::new("ShareRedemptionNode", "OWNS_SHARE_REDEMPTION");
pub const FRESH_ISSUE: RecordKind = RecordKind::new("FreshIssueNode", "OWNS_FRESH_ISSUE");
pub const CRR_REQUIREMENT: RecordKind =
    RecordKind::new("CRRRequirementNode", "OWNS_CRR_REQUIREMENT");

/// The records capital-redemption-reserve-service owns.
pub const REDEMPTION_TRANSACTION: RecordKind =
    RecordKind::new("RedemptionTransaction", "OWNS_REDEMPTION");
pub const CRR_CREATION: RecordKind = RecordKind::new("CRRCreation", "OWNS_CRR_CREATION");
pub const CRR_UTILIZATION: RecordKind = RecordKind::new("CRRUtilization", "OWNS_CRR_UTILIZATION");

/// The records share-premium-service owns.
pub const SHARE_PREMIUM_ENTRY: RecordKind =
    RecordKind::new("SharePremiumEntryNode", "OWNS_SHARE_PREMIUM_ENTRY");
pub const PREMIUM_UTILIZATION: RecordKind =
    RecordKind::new("PremiumUtilizationNode", "OWNS_PREMIUM_UTILIZATION");
pub const PREMIUM_ADJUSTMENT: RecordKind =
    RecordKind::new("PremiumAdjustmentNode", "OWNS_PREMIUM_ADJUSTMENT");

/// The records retained-profits-service owns.
pub const PROFIT_APPROPRIATION: RecordKind =
    RecordKind::new("ProfitAppropriation", "OWNS_APPROPRIATION");
pub const INTERIM_DIVIDEND: RecordKind =
    RecordKind::new("InterimDividend", "OWNS_INTERIM_DIVIDEND");
pub const PRIOR_YEAR_ADJUSTMENT: RecordKind =
    RecordKind::new("PriorYearAdjustment", "OWNS_PRIOR_ADJUSTMENT");

/// The records general-reserve-service owns.
pub const GENERAL_RESERVE: RecordKind = RecordKind::new("GeneralReserve", "OWNS_RESERVE");
pub const RESERVE_ALLOCATION: RecordKind = RecordKind::new("ReserveAllocation", "OWNS_ALLOCATION");
pub const RESERVE_UTILIZATION: RecordKind =
    RecordKind::new("ReserveUtilization", "OWNS_UTILIZATION");

/// The records bonus-shares-service and right-issues-service own.
pub const BONUS_ISSUE: RecordKind = RecordKind::new("BonusIssueNode", "OWNS_BONUS_ISSUE");
pub const RIGHT_ISSUE: RecordKind = RecordKind::new("RightIssue", "OWNS_RIGHT_ISSUE");
pub const RENUNCIATION: RecordKind = RecordKind::new("Renunciation", "OWNS_RENUNCIATION");

/// The records ordinary-shares-service and preference-shares-service own.
pub const ORDINARY_DIVIDEND: RecordKind =
    RecordKind::new("OrdinaryDividendNode", "OWNS_ORDINARY_DIVIDEND");
pub const PREFERENCE_CLASS: RecordKind =
    RecordKind::new("PreferenceShareClassNode", "OWNS_PREF_SHARE_CLASS");
pub const PREFERENCE_DIVIDEND: RecordKind =
    RecordKind::new("PreferenceDividendNode", "OWNS_PREF_DIVIDEND");
pub const PREFERENCE_REDEMPTION: RecordKind =
    RecordKind::new("PreferenceRedemptionNode", "OWNS_PREF_REDEMPTION");

/// The records authorized-share-capital-service owns.
pub const SHARE_CLASS: RecordKind = RecordKind::new("ShareClassNode", "OWNS_SHARE_CLASS");
pub const SHARE_ISSUANCE: RecordKind = RecordKind::new("ShareIssuanceNode", "OWNS_SHARE_ISSUANCE");
pub const SHARE_BUYBACK: RecordKind = RecordKind::new("ShareBuybackNode", "OWNS_SHARE_BUYBACK");

/// The records revaluation-reserve-service owns.
pub const REVALUATION_ENTRY: RecordKind =
    RecordKind::new("RevaluationEntry", "OWNS_REVALUATION_ENTRY");
pub const REVALUATION_UTILIZATION: RecordKind =
    RecordKind::new("RevaluationUtilization", "OWNS_REVALUATION_UTILIZATION");
pub const CUMULATIVE_REVALUATION: RecordKind =
    RecordKind::new("CumulativeRevaluation", "OWNS_CUMULATIVE_REVALUATION");

/// The schedules fixed-assets-schedule-service generated from that register.
pub const ASSET_SCHEDULE: RecordKind = RecordKind::new("AssetSchedule", "OWNS_ASSET_SCHEDULE");

/// The price reduction decisions selling-price-reduction-service owns.
pub const TRADING_ACCOUNT_STATEMENT: RecordKind =
    RecordKind::new("TradingAccountStatement", "OWNS_TRADING_ACCOUNT_STATEMENT");
pub const PRODUCTION_COST_STATEMENT: RecordKind =
    RecordKind::new("ProductionCostStatement", "OWNS_PRODUCTION_COST_STATEMENT");
/// The records npo-scale-service owns.
pub const NPO_SCALE_ORG: RecordKind = RecordKind::new("NPOOrg", "OWNS_NPO_SCALE_ORG");
pub const NPO_SCALE_BRANCH: RecordKind = RecordKind::new("NPOScaleBranch", "OWNS_NPO_SCALE_BRANCH");
pub const NPO_SCALE_VENDOR: RecordKind = RecordKind::new("NPOScaleVendor", "OWNS_NPO_SCALE_VENDOR");
pub const NPO_SCALE_PURCHASE: RecordKind =
    RecordKind::new("NPOScalePurchase", "OWNS_NPO_SCALE_PURCHASE");
pub const NPO_SCALE_REVENUE: RecordKind =
    RecordKind::new("NPOScaleRevenue", "OWNS_NPO_SCALE_REVENUE");
pub const NPO_SCALE_EXPENSE: RecordKind =
    RecordKind::new("NPOScaleExpense", "OWNS_NPO_SCALE_EXPENSE");

/// The records npo-scale-service owns, continued: the money it collects, pays out, and reports on.
pub const NPO_SCALE_RECEIPT: RecordKind =
    RecordKind::new("NPOScaleReceipt", "OWNS_NPO_SCALE_RECEIPT");
pub const NPO_SCALE_DONOR: RecordKind = RecordKind::new("NPOScaleDonor", "OWNS_NPO_SCALE_DONOR");
pub const NPO_SCALE_DONATION: RecordKind =
    RecordKind::new("NPOScaleDonation", "OWNS_NPO_SCALE_DONATION");
pub const NPO_SCALE_PLEDGE: RecordKind = RecordKind::new("NPOScalePledge", "OWNS_NPO_SCALE_PLEDGE");
pub const NPO_SCALE_SHAREHOLDER: RecordKind =
    RecordKind::new("NPOScaleShareholder", "OWNS_NPO_SCALE_SHAREHOLDER");
pub const NPO_SCALE_DIRECTOR: RecordKind =
    RecordKind::new("NPOScaleDirector", "OWNS_NPO_SCALE_DIRECTOR");
pub const NPO_SCALE_DIVIDEND: RecordKind =
    RecordKind::new("NPOScaleDividend", "OWNS_NPO_SCALE_DIVIDEND");
pub const NPO_SCALE_PARTNER: RecordKind =
    RecordKind::new("NPOScalePartner", "OWNS_NPO_SCALE_PARTNER");
pub const NPO_SCALE_PARTNER_DRAW: RecordKind =
    RecordKind::new("NPOScalePartnerDraw", "OWNS_NPO_SCALE_PARTNER_DRAW");
pub const NPO_SCALE_BUDGET: RecordKind = RecordKind::new("NPOScaleBudget", "OWNS_NPO_SCALE_BUDGET");
pub const NPO_SCALE_COMPLIANCE: RecordKind =
    RecordKind::new("NPOScaleCompliance", "OWNS_NPO_SCALE_COMPLIANCE");
pub const NPO_SCALE_BALANCE_ITEM: RecordKind =
    RecordKind::new("NPOScaleBalanceItem", "OWNS_NPO_SCALE_BALANCE_ITEM");

/// The records npo-service owns.
pub const NPO_FUND: RecordKind = RecordKind::new("NPOFund", "OWNS_NPO_FUND");
pub const NPO_FUND_TRANSACTION: RecordKind =
    RecordKind::new("NPOFundTransaction", "OWNS_NPO_FUND_TRANSACTION");
pub const NPO_FUND_RESTRICTION: RecordKind =
    RecordKind::new("FundRestriction", "OWNS_FUND_RESTRICTION");
pub const NPO_NET_ASSETS: RecordKind = RecordKind::new("NetAssets", "OWNS_NET_ASSETS");
pub const NPO_DONATION: RecordKind = RecordKind::new("Donation", "OWNS_DONATION");
pub const NPO_GRANT: RecordKind = RecordKind::new("Grant", "OWNS_GRANT");
pub const NPO_GRANT_DRAWDOWN: RecordKind = RecordKind::new("GrantDrawdown", "OWNS_GRANT_DRAWDOWN");
pub const NPO_PROJECT: RecordKind = RecordKind::new("Project", "OWNS_PROJECT");
pub const NPO_PROGRAM: RecordKind = RecordKind::new("Program", "OWNS_PROGRAM");
pub const NPO_DONOR: RecordKind = RecordKind::new("Donor", "OWNS_DONOR");
pub const NPO_BUDGET: RecordKind = RecordKind::new("Budget", "OWNS_BUDGET");
pub const NPO_BUDGET_LINE: RecordKind = RecordKind::new("BudgetLine", "OWNS_BUDGET_LINE");
pub const NPO_INTERNAL_CONTROL: RecordKind =
    RecordKind::new("InternalControl", "OWNS_INTERNAL_CONTROL");
pub const NPO_AUDIT_REPORT: RecordKind = RecordKind::new("AuditReport", "OWNS_AUDIT_REPORT");
pub const NPO_PROGRAM_METRIC: RecordKind = RecordKind::new("ProgramMetric", "OWNS_PROGRAM_METRIC");
pub const NPO_IMPACT_MEASUREMENT: RecordKind =
    RecordKind::new("ImpactMeasurement", "OWNS_IMPACT_MEASUREMENT");
pub const NPO_VOLUNTEER_RECORD: RecordKind =
    RecordKind::new("VolunteerRecord", "OWNS_VOLUNTEER_RECORD");
pub const NPO_STATEMENT_OF_ACTIVITIES: RecordKind =
    RecordKind::new("StatementOfActivities", "OWNS_STATEMENT_OF_ACTIVITIES");
pub const NPO_STATEMENT_OF_POSITION: RecordKind =
    RecordKind::new("StatementOfFinancialPosition", "OWNS_STATEMENT_OF_FP");

// payables: the purchases ledger and the expenses claimed against it
pub const CREDITOR_TRANSACTION: RecordKind =
    RecordKind::new("CreditorTransaction", "OWNS_CREDITOR_TX");
pub const EXPENSE: RecordKind = RecordKind::new("Expense", "SUBMITTED");

/// The records family-community-group-service owns.
pub const COMMUNITY_GROUP: RecordKind = RecordKind::new("CommunityGroup", "OWNS_GROUP");
pub const GROUP_MEMBER: RecordKind = RecordKind::new("GroupMember", "OWNS_MEMBER");
pub const GROUP_CONTRIBUTION: RecordKind =
    RecordKind::new("GroupContribution", "OWNS_CONTRIBUTION");
pub const GROUP_PAYOUT: RecordKind = RecordKind::new("GroupPayout", "OWNS_PAYOUT");

pub const PARTNERSHIP_AGREEMENT: RecordKind =
    RecordKind::new("PartnershipAgreement", "OWNS_PARTNERSHIP_AGREEMENT");
pub const PARTNER_CHANGE: RecordKind = RecordKind::new("PartnerChange", "OWNS_PARTNER_CHANGE");
pub const DISSOLUTION_REPORT: RecordKind =
    RecordKind::new("DissolutionReport", "OWNS_DISSOLUTION_REPORT");
pub const PARTNERSHIP_REVALUATION: RecordKind =
    RecordKind::new("PartnershipRevaluation", "OWNS_PARTNERSHIP_REVALUATION");
// receivables: the sales ledger, the debts that go bad in it and the provisions against them
pub const BAD_DEBT_RECORD: RecordKind = RecordKind::new("BadDebtRecord", "OWNS_BAD_DEBT");
pub const RECOVERY_JOURNAL: RecordKind =
    RecordKind::new("RecoveryJournal", "OWNS_RECOVERY_JOURNAL");
pub const PROVISION_POLICY: RecordKind =
    RecordKind::new("ProvisionPolicy", "OWNS_PROVISION_POLICY");
pub const DEBTOR_RECORD: RecordKind = RecordKind::new("DebtorRecord", "OWNS_DEBTOR");
pub const PROVISION_JOURNAL: RecordKind =
    RecordKind::new("ProvisionJournal", "OWNS_PROVISION_JOURNAL");
pub const DOUBTFUL_DEBTOR: RecordKind = RecordKind::new("DoubtfulDebtor", "OWNS_DOUBTFUL_DEBTOR");
pub const DOUBTFUL_PROVISION: RecordKind =
    RecordKind::new("DoubtfulDebtProvision", "OWNS_DOUBTFUL_PROVISION");
pub const DEBTOR_TRANSACTION: RecordKind =
    RecordKind::new("DebtorTransaction", "OWNS_DEBTOR_TRANSACTION");
// the ledger kernel's services: the breaks in a control account, the integrity checks, the documents
pub const RECONCILIATION_ERROR: RecordKind =
    RecordKind::new("ReconciliationError", "OWNS_RECONCILIATION_ERROR");
pub const RECONCILIATION_REPORT: RecordKind =
    RecordKind::new("ReconciliationReport", "OWNS_RECONCILIATION_REPORT");
pub const INTEGRITY_CHECK: RecordKind = RecordKind::new("IntegrityCheck", "OWNS_INTEGRITY_CHECK");
pub const FINANCIAL_DOCUMENT: RecordKind =
    RecordKind::new("FinancialDocument", "OWNS_FINANCIAL_DOCUMENT");
pub const ACTIVITY: RecordKind = RecordKind::new("Activity", "OWNS_ACTIVITY");
pub const ACTIVITY_BUDGET: RecordKind = RecordKind::new("ActivityBudget", "OWNS_ACTIVITY_BUDGET");
pub const SELLING_PRICE_ANALYSIS: RecordKind =
    RecordKind::new("PriceReductionAnalysis", "OWNS_PRICE_ANALYSIS");
// reporting: the templates a Book keeps, the schedules they run on, and the reports they produced
pub const REPORT_TEMPLATE: RecordKind = RecordKind::new("ReportTemplate", "OWNS_REPORT_TEMPLATE");
pub const REPORT_SCHEDULE: RecordKind = RecordKind::new("ReportSchedule", "OWNS_REPORT_SCHEDULE");
pub const GENERATED_REPORT: RecordKind =
    RecordKind::new("GeneratedReport", "OWNS_GENERATED_REPORT");
// reporting, distribution: the lists a report is addressed to, the subscriptions that send it, and the record of
// what was sent
pub const DISTRIBUTION_LIST: RecordKind =
    RecordKind::new("DistributionList", "OWNS_DISTRIBUTION_LIST");
pub const REPORT_SUBSCRIPTION: RecordKind =
    RecordKind::new("ReportSubscription", "OWNS_SUBSCRIPTION");
/// One line of a double entry: a real record of its own, not a block inside the entry, so a line can be found by the
/// account it moves without reading every entry first. Written and read with its entry in a single statement, because
/// a line that exists without the entry it belongs to is a fragment of a posting.
pub const JOURNAL_LINE: RecordKind = RecordKind::new("JournalLine", "HAS_LINE");

pub const DISTRIBUTION_LOG: RecordKind =
    RecordKind::new("DistributionLog", "OWNS_DISTRIBUTION_LOG");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The record does not exist, or belongs to another Book. Deliberately the same answer.
    NotFound,
    /// The caller asked for something that is not theirs to ask for - another person's inbox, say. The answer a
    /// shipped service gave as a 403.
    Forbidden(String),
    Graph(String),
    Invalid(String),
    /// The record contradicts one already held - the same external id imported twice, say. The answer a
    /// shipped service gave as a 409.
    Conflict(String),
    /// The request was understood and refused on its merits - a backup type that does not exist, a restore asked of a
    /// backup that never finished. The answer a shipped service gave as a 400, kept distinct from the 422 a malformed
    /// body earns because a caller telling the two apart is what makes the answer actionable.
    BadRequest(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "record not found"),
            Self::Forbidden(message) => write!(f, "{message}"),
            Self::Graph(message) => write!(f, "graph error: {message}"),
            Self::Invalid(message) => write!(f, "{message}"),
            Self::Conflict(message) => write!(f, "{message}"),
            Self::BadRequest(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// The one thing the store needs from a database: run a parameterised statement, get rows back.
///
/// Keeping this to a single method is what lets the store be tested without a server, and keeps
/// the driver quiet in exactly one file.
pub trait Graph {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError>;
}

/// Book-scoped CRUD over one record kind.
pub struct Records<'a, G: Graph + ?Sized> {
    graph: &'a G,
    kind: RecordKind,
}

impl<'a, G: Graph + ?Sized> Records<'a, G> {
    pub fn new(graph: &'a G, kind: RecordKind) -> Self {
        Self { graph, kind }
    }

    /// Store a new record, stamped with the caller's scope.
    ///
    /// A payload that tries to set `user_id` or `book_id` is rejected rather than silently
    /// overwritten: a caller that thinks it is writing somewhere else should hear about it.
    pub fn create(&self, scope: &BookScope, fields: &Value) -> Result<Value, StoreError> {
        let mut props = self.clean_fields(fields)?;
        props.insert("user_id".into(), json!(scope.user_id));
        props.insert("book_id".into(), json!(scope.book_id));
        let id = props
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| uuid_like(&scope.book_id, props.len()));
        props.insert("id".into(), json!(id));

        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})\n\
             CREATE (x:{label})\n\
             SET x = $props\n\
             CREATE (u)-[:{edge}]->(x)\n\
             RETURN x",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["props"] = Value::Object(props);
        self.one(cypher, params)
    }

    /// Store a record together with the children it owns, in ONE statement.
    ///
    /// A child is not a list inside its parent: it is a record of its own, joined to the parent by an edge, so it can
    /// be found by what it says rather than by reading every parent first. It is written in the same statement as the
    /// parent because two writes could leave a posting existing in halves, which in a ledger is not a state that may
    /// exist.
    pub fn create_with_children(
        &self,
        scope: &BookScope,
        fields: &Value,
        child: RecordKind,
        children: &[Value],
        // the property each child carries the parent's id under - `entry_id` for a journal line, and whatever a
        // different record calls its owner. The store assigns the id, so it must stamp the children itself.
        parent_key: &str,
    ) -> Result<Value, StoreError> {
        let mut props = self.clean_fields(fields)?;
        let id = props
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| uuid_like(&scope.book_id, props.len()));
        props.insert("id".into(), json!(id));
        props.insert("user_id".into(), json!(scope.user_id));
        props.insert("book_id".into(), json!(scope.book_id));

        let mut held: Vec<Value> = Vec::with_capacity(children.len());
        for (index, line) in children.iter().enumerate() {
            let mut child_props = line
                .as_object()
                .cloned()
                .ok_or_else(|| StoreError::Invalid("a child must be an object".into()))?;
            if child_props.contains_key("user_id") || child_props.contains_key("book_id") {
                return Err(StoreError::Invalid(
                    "user_id and book_id come from the caller's scope, not the payload".into(),
                ));
            }
            child_props.insert("user_id".into(), json!(scope.user_id));
            child_props.insert("book_id".into(), json!(scope.book_id));
            child_props.insert("id".into(), json!(uuid_like(&scope.book_id, index)));
            child_props.insert(parent_key.to_string(), json!(id));
            held.push(Value::Object(child_props));
        }

        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})\n\
             CREATE (x:{label})\n\
             SET x = $props\n\
             CREATE (u)-[:{edge}]->(x)\n\
             WITH x\n\
             UNWIND $children AS child\n\
             CREATE (l:{child_label})\n\
             SET l = child\n\
             MERGE (x)-[:{child_edge}]->(l)\n\
             WITH x, l, child\n\
             OPTIONAL MATCH (a:Account {{book_id: $book_id, account_number: child.account_number}})\n\
             FOREACH (_ IN CASE WHEN child.side = 'debit' THEN [1] ELSE [] END |\n\
                 MERGE (l)-[:DEBITED]->(a))\n\
             FOREACH (_ IN CASE WHEN child.side = 'credit' THEN [1] ELSE [] END |\n\
                 MERGE (l)-[:CREDITED]->(a))\n\
             WITH x, l\n\
             RETURN x, collect(l) AS lines",
            label = self.kind.label,
            edge = self.kind.edge,
            child_label = child.label,
            child_edge = child.edge
        );
        let mut params = scope.params();
        params["props"] = Value::Object(props);
        params["children"] = Value::Array(held);
        let row = self.one(cypher, params)?;
        Ok(with_children(row, "lines"))
    }

    /// One record and the children it owns, read in ONE statement.
    pub fn get_with_children(
        &self,
        scope: &BookScope,
        id: &str,
        child: RecordKind,
    ) -> Result<Value, StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.id = $id AND x.book_id = $book_id\n\
             OPTIONAL MATCH (x)-[:{child_edge}]->(l:{child_label})\n\
             RETURN x, collect(l) AS lines",
            label = self.kind.label,
            edge = self.kind.edge,
            child_edge = child.edge,
            child_label = child.label
        );
        let mut params = scope.params();
        params["id"] = json!(id);
        let row = self.one(cypher, params)?;
        Ok(with_children(row, "lines"))
    }

    /// Every record of this Book under this label, each with the children it owns, read in ONE statement.
    pub fn list_with_children(
        &self,
        scope: &BookScope,
        limit: usize,
        child: RecordKind,
    ) -> Result<Vec<Value>, StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.book_id = $book_id\n\
             OPTIONAL MATCH (x)-[:{child_edge}]->(l:{child_label})\n\
             RETURN x, collect(l) AS lines\n\
             ORDER BY x.created_at DESC\n\
             LIMIT $limit",
            label = self.kind.label,
            edge = self.kind.edge,
            child_edge = child.edge,
            child_label = child.label
        );
        let mut params = scope.params();
        params["limit"] = json!(limit as i64);
        let rows = self.graph.run(&cypher, params)?;
        Ok(rows
            .into_iter()
            .map(|row| with_children(row, "lines"))
            .collect())
    }

    pub fn get(&self, scope: &BookScope, id: &str) -> Result<Value, StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.id = $id AND x.book_id = $book_id\n\
             RETURN x",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["id"] = json!(id);
        self.one(cypher, params)
    }

    /// A public verification token is itself the credential, so this is the one read that crosses Books.
    ///
    /// A receipt number and a holding code are printed on a document and handed to a donor or an investor who has no
    /// account here. Looking one up has to ignore the Book scoping every other read keeps, so it is kept to exactly
    /// that: one record whose named property is the token the caller already holds, and nothing else.
    pub fn by_token(&self, property: &str, token: &str) -> Result<Option<Value>, StoreError> {
        if token.trim().is_empty() {
            return Ok(None);
        }
        let cypher = format!(
            "MATCH (x:{label})\nWHERE x.{property} = $token\nRETURN x",
            label = self.kind.label,
            property = property
        );
        let rows = self.graph.run(&cypher, json!({ "token": token }))?;
        Ok(rows.into_iter().next().map(unwrap_node))
    }

    pub fn list(&self, scope: &BookScope, limit: usize) -> Result<Vec<Value>, StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.book_id = $book_id\n\
             RETURN x\n\
             ORDER BY x.created_at DESC\n\
             LIMIT $limit",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["limit"] = json!(limit as i64);
        let rows = self.graph.run(&cypher, params)?;
        Ok(rows.into_iter().map(unwrap_node).collect())
    }

    /// The lines that moved one account, fetched through the edges rather than by reading every entry in the Book.
    ///
    /// This is what the `DEBITED`/`CREDITED` edges are for: an account's postings are a walk from the account inwards,
    /// so answering "what moved account 1000" stops depending on how many entries the Book holds. Each row is the line
    /// itself with the entry it belongs to named beside it.
    pub fn lines_for_account(
        &self,
        scope: &BookScope,
        child: RecordKind,
        account_number: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             MATCH (x)-[:{child_edge}]->(l:{child_label})\n\
             MATCH (l)-[:DEBITED|CREDITED]->(a:Account {{book_id: $book_id, account_number: $account_number}})\n\
             RETURN l, x.id AS entry_id, x.entry_date AS entry_date, x.description AS entry_description\n\
             ORDER BY entry_date DESC\n\
             LIMIT $limit",
            label = self.kind.label,
            edge = self.kind.edge,
            child_edge = child.edge,
            child_label = child.label
        );
        let mut params = scope.params();
        params["account_number"] = json!(account_number);
        params["limit"] = json!(limit as i64);
        self.graph.run(&cypher, params)
    }

    /// Merge the supplied fields into a record that already belongs to this Book.
    pub fn update(&self, scope: &BookScope, id: &str, fields: &Value) -> Result<Value, StoreError> {
        let patch = self.clean_fields(fields)?;
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.id = $id AND x.book_id = $book_id\n\
             SET x += $patch\n\
             RETURN x",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["id"] = json!(id);
        params["patch"] = Value::Object(patch);
        self.one(cypher, params)
    }

    pub fn delete(&self, scope: &BookScope, id: &str) -> Result<(), StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.id = $id AND x.book_id = $book_id\n\
             DETACH DELETE x\n\
             RETURN count(x) AS deleted",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["id"] = json!(id);
        let rows = self.graph.run(&cypher, params)?;
        // DETACH DELETE returns no row for a miss, so an empty answer is the same NotFound as get.
        if rows.is_empty() {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    fn clean_fields(&self, fields: &Value) -> Result<Map<String, Value>, StoreError> {
        let object = fields
            .as_object()
            .ok_or_else(|| StoreError::Invalid("a record must be a JSON object".into()))?;
        if object.contains_key("user_id") || object.contains_key("book_id") {
            return Err(StoreError::Invalid(
                "user_id and book_id come from the caller's scope, not the payload".into(),
            ));
        }
        Ok(object.clone())
    }

    fn one(&self, cypher: impl AsRef<str>, params: Value) -> Result<Value, StoreError> {
        let rows = self.graph.run(cypher.as_ref(), params)?;
        rows.into_iter()
            .next()
            .map(unwrap_node)
            .ok_or(StoreError::NotFound)
    }
}

/// A row is `{"x": {...}}`; the record is what is inside it.
pub(crate) fn unwrap_node(row: Value) -> Value {
    if let Some(object) = row.as_object() {
        if object.len() == 1 {
            if let Some((_, node)) = object.iter().next() {
                return node.clone();
            }
        }
    }
    row
}

/// The one shape a shareholder row has, whichever service registers it.
///
/// issued-share-capital-service and company-accounting-service both keep a company's shareholder
/// register, and both write `:ShareholderNode`. They used to write different fields for the same
/// facts (`name` in one, `shareholder_name` in the other), which would have made the register depend
/// on which service had written the row. Both now write this shape, and both accept either spelling
/// on the way in.
pub fn shareholder_record(payload: &Value) -> Value {
    let name = payload
        .get("shareholder_name")
        .or_else(|| payload.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    serde_json::json!({
        "company_id": payload.get("company_id").and_then(Value::as_str).unwrap_or_default(),
        "shareholder_name": name,
        "shareholder_type": payload
            .get("shareholder_type")
            .and_then(Value::as_str)
            .unwrap_or("individual"),
        "share_class": payload.get("share_class").and_then(Value::as_str).unwrap_or("ordinary"),
        "shares_held": payload.get("shares_held").and_then(Value::as_i64).unwrap_or_default(),
        "percentage_holding": payload
            .get("percentage_holding")
            .and_then(Value::as_f64)
            .unwrap_or_default(),
        "registration_date": payload
            .get("registration_date")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "address": payload.get("address").and_then(Value::as_str).unwrap_or_default(),
        "tax_status": payload.get("tax_status").and_then(Value::as_str).unwrap_or_default(),
        "is_controlling_party": payload
            .get("is_controlling_party")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// A created record, whichever shape the driver returned it in.
///
/// The driver answers `CREATE ... RETURN x` with the node, so a record comes back as an object. A test
/// double may answer with the bare id instead. Reading the record through this function means a port
/// does not have to know which happened, and returning it to a caller does not depend on it either.
/// A row whose children were collected under `key`, with the empty ones dropped.
///
/// `collect()` answers `[null]` for a parent with no children, so an empty list is what that means - never a list
/// holding a null, which a reader would take for a child with no fields.
fn with_children(mut row: Value, key: &str) -> Value {
    let held = match row.get(key).cloned() {
        Some(Value::Array(items)) => {
            Value::Array(items.into_iter().filter(|item| !item.is_null()).collect())
        }
        _ => Value::Array(Vec::new()),
    };
    if let Some(object) = row.as_object_mut() {
        object.insert(key.to_string(), held);
    }
    row
}

pub fn created_record(created: &Value) -> Value {
    match created {
        Value::String(id) => serde_json::json!({ "id": id }),
        other => other.clone(),
    }
}

/// The id of a created record, whichever shape the driver returned it in.
pub fn created_id(created: &Value) -> Value {
    created_record(created)
        .get("id")
        .cloned()
        .unwrap_or(Value::Null)
}

/// A stable local id. Not a real UUID: the store only needs uniqueness within a Book, and
/// pulling in a uuid dependency for that is not worth it.
fn uuid_like(book_id: &str, salt: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mixed = (nanos as u64) ^ ((salt as u64) << 32) ^ book_id.len() as u64;
    format!("{mixed:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records every statement so the test can assert on the Cypher and the parameters.
    struct FakeGraph {
        rows: RefCell<Vec<Value>>,
        seen: RefCell<Vec<(String, Value)>>,
    }

    impl FakeGraph {
        fn returning(rows: Vec<Value>) -> Self {
            Self {
                rows: RefCell::new(rows),
                seen: RefCell::new(Vec::new()),
            }
        }

        fn last(&self) -> (String, Value) {
            self.seen
                .borrow()
                .last()
                .cloned()
                .expect("no statement ran")
        }
    }

    impl Graph for FakeGraph {
        fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
            self.seen.borrow_mut().push((cypher.to_owned(), params));
            Ok(self.rows.borrow().clone())
        }
    }

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_new_record_is_stamped_with_the_callers_scope() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "abc", "book_id": "book-1"}})]);
        let store = Records::new(&graph, PRODUCT_COST);
        let saved = store
            .create(&scope(), &json!({"name": "Menswear (Q3)", "cost": 1200.0}))
            .unwrap();
        assert_eq!(saved["book_id"], json!("book-1"));

        let (cypher, params) = graph.last();
        assert!(cypher.contains("CREATE (x:ProductCost)"), "{cypher}");
        assert!(cypher.contains("[:OWNS_PRODUCT_COST]"), "{cypher}");
        assert_eq!(params["props"]["user_id"], json!("user-1"));
        assert_eq!(params["props"]["book_id"], json!("book-1"));
    }

    #[test]
    fn a_payload_cannot_choose_its_own_book() {
        let graph = FakeGraph::returning(vec![]);
        let store = Records::new(&graph, PRODUCT_COST);
        let err = store
            .create(&scope(), &json!({"book_id": "someone-elses-book"}))
            .unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{err:?}");
        assert_eq!(
            graph.seen.borrow().len(),
            0,
            "nothing should reach the database"
        );
    }

    #[test]
    fn values_never_land_in_the_statement_text() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "abc"}})]);
        let store = Records::new(&graph, PRODUCT_COST);
        let tricky = "Menswear'); MATCH (n) DETACH DELETE n //";
        store.create(&scope(), &json!({"name": tricky})).unwrap();
        let (cypher, params) = graph.last();
        assert!(!cypher.contains("Menswear"), "the value was interpolated");
        assert_eq!(params["props"]["name"], json!(tricky));
    }

    #[test]
    fn reads_are_confined_to_the_callers_book() {
        let graph = FakeGraph::returning(vec![]);
        let store = Records::new(&graph, JOB);
        assert_eq!(
            store.get(&scope(), "job-9").unwrap_err(),
            StoreError::NotFound
        );
        let (cypher, params) = graph.last();
        assert!(cypher.contains("x.book_id = $book_id"), "{cypher}");
        assert_eq!(params["book_id"], json!("book-1"));
        assert_eq!(params["user_id"], json!("user-1"));
    }

    #[test]
    fn an_update_cannot_move_a_record_between_books() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "job-9"}})]);
        let store = Records::new(&graph, JOB);
        let err = store
            .update(&scope(), "job-9", &json!({"book_id": "book-2"}))
            .unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{err:?}");
    }

    #[test]
    fn a_delete_that_matches_nothing_is_a_miss_not_a_success() {
        let graph = FakeGraph::returning(vec![]);
        let store = Records::new(&graph, JOB_COST_ENTRY);
        assert_eq!(
            store.delete(&scope(), "entry-1").unwrap_err(),
            StoreError::NotFound
        );
    }

    #[test]
    fn listing_is_newest_first_and_bounded() {
        let graph =
            FakeGraph::returning(vec![json!({"x": {"id": "a"}}), json!({"x": {"id": "b"}})]);
        let store = Records::new(&graph, OVERHEAD_ABSORPTION);
        let rows = store.list(&scope(), 50).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], json!("a"));
        let (cypher, params) = graph.last();
        assert!(cypher.contains("ORDER BY x.created_at DESC"), "{cypher}");
        assert_eq!(params["limit"], json!(50));
    }
}
