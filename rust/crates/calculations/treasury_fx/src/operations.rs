//! Treasury operations: transaction and cash-position analysis.
//!
//! Amounts are [`Money`], not `f64`, because these totals are the figures a
//! treasurer acts on and a group balance can run to dozens of digits.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use vimbai_common::Money;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreasuryTransaction {
    pub transaction_id: String,
    /// `"inflow"` or `"outflow"`. Anything else counts as neither.
    pub transaction_type: String,
    pub amount: Money,
    pub currency: String,
    pub counterparty: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreasuryOperationsRequest {
    pub company_id: String,
    pub transactions: Vec<TreasuryTransaction>,
    pub bank_balances: BTreeMap<String, Money>,
    pub treasury_policy_limits: BTreeMap<String, Money>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionsSummary {
    pub total_transactions: usize,
    pub total_inflows: Money,
    pub total_outflows: Money,
    pub net_flow: Money,
    pub pending: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyCompliance {
    pub limits_defined: usize,
    /// A flat 0.95. Nothing in the source ever compares a transaction with a
    /// limit, so this number is a constant and not a measurement.
    pub compliance_rate: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BankBalanceSummary {
    pub total_cash: Money,
    pub bank_count: usize,
    pub largest_balance: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreasuryOperationsResult {
    pub company_id: String,
    pub total_cash: Money,
    pub transactions_summary: TransactionsSummary,
    pub policy_compliance: PolicyCompliance,
    pub bank_balance_summary: BankBalanceSummary,
    pub alerts: Vec<String>,
    pub recommendations: Vec<String>,
}

/// The cash floor below which the source recommends topping up liquidity.
const LOW_CASH_THRESHOLD: &str = "1000000";
/// Failure rate above which an alert is raised (strictly greater, so exactly
/// 5% is silent).
const FAILURE_ALERT_SHARE: &str = "0.05";
/// Pending count above which an alert is raised.
const PENDING_ALERT_COUNT: usize = 20;

/// `/analyze` on the treasury operations service.
///
/// PYTHON QUIRKS, preserved:
/// - every transaction is summed into inflow or outflow by its `type` string
///   alone; `currency` and `status` never filter a total, so a failed outflow
///   still reduces the net flow;
/// - the failure alert compares a count against a float percentage of the
///   total, so a book of 19 transactions needs one failure to trip it
///   (1 > 0.95) but a book of 21 needs two;
/// - `compliance_rate` is the constant 0.95 whatever the limits say;
/// - `largest_balance` on an empty map is zero, not an error;
/// - recommendations are keyed off `< 1,000,000` cash and any failed
///   transaction at all, independent of the alerts above.
pub fn analyze_treasury_operations(req: &TreasuryOperationsRequest) -> TreasuryOperationsResult {
    let total_cash = Money::sum(req.bank_balances.values());

    let inflows = req
        .transactions
        .iter()
        .filter(|t| t.transaction_type == "inflow")
        .map(|t| &t.amount);
    let outflows = req
        .transactions
        .iter()
        .filter(|t| t.transaction_type == "outflow")
        .map(|t| &t.amount);
    let total_inflows = Money::sum(inflows);
    let total_outflows = Money::sum(outflows);

    let pending = req
        .transactions
        .iter()
        .filter(|t| t.status == "pending")
        .count();
    let failed = req
        .transactions
        .iter()
        .filter(|t| t.status == "failed")
        .count();

    let mut alerts = Vec::new();
    // failed > total * 0.05, computed exactly rather than in binary float
    let failure_threshold = Money::from_i64(req.transactions.len() as i64)
        .times(&Money::parse(FAILURE_ALERT_SHARE).expect("constant"));
    if Money::from_i64(failed as i64) > failure_threshold {
        alerts.push(format!("High failure rate: {failed} failed transactions"));
    }
    if pending > PENDING_ALERT_COUNT {
        alerts.push(format!("Many pending transactions: {pending}"));
    }

    let largest = req
        .bank_balances
        .values()
        .max()
        .cloned()
        .unwrap_or_else(Money::zero);

    let mut recommendations = Vec::new();
    if total_cash < Money::parse(LOW_CASH_THRESHOLD).expect("constant") {
        recommendations.push("Cash position is low - ensure adequate liquidity".to_string());
    }
    if failed > 0 {
        recommendations.push("Review failed transactions and resolve issues".to_string());
    }

    TreasuryOperationsResult {
        company_id: req.company_id.clone(),
        total_cash: total_cash.round(2),
        transactions_summary: TransactionsSummary {
            total_transactions: req.transactions.len(),
            total_inflows: total_inflows.round(2),
            total_outflows: total_outflows.round(2),
            net_flow: total_inflows.minus(&total_outflows).round(2),
            pending,
            failed,
        },
        policy_compliance: PolicyCompliance {
            limits_defined: req.treasury_policy_limits.len(),
            compliance_rate: Money::parse("0.95").expect("constant"),
        },
        bank_balance_summary: BankBalanceSummary {
            total_cash: total_cash.round(2),
            bank_count: req.bank_balances.len(),
            largest_balance: largest.round(2),
        },
        alerts,
        recommendations,
    }
}
