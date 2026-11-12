//! Accounting services: the cash flow statement, the trading account and suspense analysis. Statements that read recorded figures and compute the ones they publish, plus the corrections that clear a suspense balance.
//!
//! Services here:
//! * `cash-flow-statement-service`
//! * `departmental-accounting-service` (departments, cost pools, allocation, inter-department bills)
//! * `company-accounting-service` (companies, their register, capital, dividends, reserves, reports)
//! * `cashbook-service` (cash accounts, their entries, bank reconciliations, cash flow and position)
//! * `ifrs-reporting-service` (IFRS reports, their figures, and their disclosure notes)
//! * `intercompany-service` (group entities, the transactions between them, and their eliminations)
//! * `cash-flow-service` (a project's yearly flows, kept, with the standalone flow calculators)
//! * `balance-sheet-service` (the sheet, its three totals, and the ratios read off the latest)
//! * `fund-accounting-service` (funds with their derived net assets, and the transactions that
//!   re-derive them)
//! * `revenue-recognition-service` (contracts with their obligations, the allocation by standalone selling
//!   price, and recognition against one obligation at a time)
//! * `suspense-error-service`
//! * `trading-account-service`
//!
//! One crate per business domain, not one per service: these services were separate Python
//! containers that each carried a copy of the same models, the same database access and the same
//! arithmetic. Grouped, a domain shares one build, one test run and one place to look.

#![forbid(unsafe_code)]

pub mod balance_sheet;
pub mod cash_flow;
pub mod cash_flow_schedule;
pub mod cashbook;
pub mod company_accounting;
pub mod departments;
pub mod fund_accounting;
pub mod ifrs_reports;
pub mod intercompany;
pub mod revenue_recognition;
pub mod suspense;
pub mod trading_account;
pub use balance_sheet::{
    generate as generate_balance_sheet, history as balance_sheet_history, latest_sheet,
    ratios as balance_sheet_ratios, totals as balance_sheet_totals,
};
pub use cash_flow_schedule::{
    build_entries as build_cash_flow_entries, create as create_cash_flow_schedule,
    cumulative_flows, get as get_cash_flow_schedule, list as list_cash_flow_schedules, net_flows,
    pair as pair_flows, total_of as cash_flow_total,
};
pub use cashbook::{
    cash_flow_summary, cash_position, cash_position_history, complete_reconciliation,
    create_account as create_cash_account, create_cash_flow_entry,
    create_entry as create_cash_entry, create_reconciliation as create_cash_reconciliation,
    get_account as get_cash_account, get_entry as get_cash_entry,
    get_reconciliation as get_cash_reconciliation, list_accounts as list_cash_accounts,
    list_cash_flow, list_entries as list_cash_entries,
    list_reconciliations as list_cash_reconciliations,
    outstanding_items as cash_reconciliation_outstanding, post_entry as post_cash_entry,
    summary as cashbook_summary, update_account as update_cash_account,
    void_entry as void_cash_entry,
};
pub use company_accounting::{
    create_company, create_reserve as create_company_reserve, create_share_capital,
    declare_dividend, dividend_history, dividend_payments, equity_statement, get_company,
    list_capital_transactions, list_companies, list_dividends, list_share_capital,
    list_shareholders, pay_dividend, record_capital_transaction, record_retained_earnings,
    register_shareholder, reserves as company_reserves, retained_earnings, shareholder_register,
    update_company,
};
pub use departments::{
    allocation_results, approve_inter_department_bill, compare_departments, cost_distribution,
    create_allocation_rule, create_cost_pool, create_department, create_inter_department_bill,
    deactivate_allocation_rule, department_financials, department_hierarchy, get_cost_pool,
    get_department, list_allocation_rules, list_cost_pools, list_departments,
    list_inter_department_bills, performance_report, run_allocation, update_department,
};
pub use fund_accounting::{
    create_fund, list_funds, list_transactions as list_fund_transactions,
    net_assets as fund_net_assets, record_transaction as record_fund_transaction,
    totals as fund_totals,
};
pub use ifrs_reports::{
    add_note as add_ifrs_note, approve_report as approve_ifrs_report,
    create_report as create_ifrs_report, get_report as get_ifrs_report,
    list_notes as list_ifrs_notes, list_reports as list_ifrs_reports,
};
pub use intercompany::{
    create_entity as create_intercompany_entity,
    create_transaction as create_intercompany_transaction, list_eliminations,
    list_entities as list_intercompany_entities,
    list_transactions as list_intercompany_transactions,
    match_transactions as match_intercompany_transactions,
};
pub use revenue_recognition::{
    allocate_price, create_contract as create_revenue_contract,
    list_contracts as list_revenue_contracts, recognise, recognise_revenue, revenue_summary,
};
