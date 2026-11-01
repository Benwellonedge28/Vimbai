#![forbid(unsafe_code)]
//! Payroll and equity math: payroll runs, statutory taxes, payroll
//! accounting journals, earnings per share (basic/diluted).
//!
//! Ported from: payroll-service, payroll-accounting-service and
//! eps-service. Queued for this crate: partnership-accounting,
//! partnership-sale, share-options, pension-accounting, bonus-shares,
//! right-issues, share-capital family, retained-profits.

pub mod eps;
pub mod partnership;
pub mod payroll;
pub mod payroll_accounting;
pub mod pension;
pub mod share_capital;
pub mod share_options;

pub use eps::{calculate_eps, AntiDilutiveItem, EpsOutput, OptionInput};
pub use partnership::{allocate_profit, AllocationOutput, PartnerAccount, PartnerInput};
pub use payroll::{
    payroll_run, payroll_taxes, EmployeeInput, EmployeePay, PayrollRunOutput, PayrollTaxOutput,
};
pub use payroll_accounting::{
    process_payroll, EmployeeSalary, JournalLine, PayrollAccountingOutput,
};
