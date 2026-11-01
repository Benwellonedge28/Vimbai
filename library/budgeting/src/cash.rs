#![forbid(unsafe_code)]
//! Cash budget, ported from cash-budget-service `/prepare`.
//! 12 monthly periods; quarterly timing codes ("Q1".."Q4") determine
//! which months a receipt/payment lands in.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashFlowItem {
    pub amount: f64,
    /// Timing marker; contains a quarterly code such as "Q1".
    pub timing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashBudgetRequest {
    pub company_id: String,
    pub budget_period: String,
    pub opening_cash: f64,
    pub minimum_cash_balance: f64,
    #[serde(default)]
    pub expected_receipts: Vec<CashFlowItem>,
    #[serde(default)]
    pub expected_payments: Vec<CashFlowItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashBudgetPeriod {
    pub period: String,
    pub opening_balance: f64,
    pub receipts: f64,
    pub payments: f64,
    pub closing_balance: f64,
    pub financing: f64,
    pub cumulative_cash: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashBudgetResult {
    pub company_id: String,
    pub budget_period: String,
    pub periods: Vec<CashBudgetPeriod>,
    pub total_receipts: f64,
    pub total_payments: f64,
    pub net_cash_flow: f64,
    pub minimum_cash_balance: f64,
    pub peak_financing_required: f64,
    pub total_financing_cost: f64,
}

/// Cash budget (cash-budget-service `/prepare`). Financing tops the
/// closing balance up to the minimum; `cumulative_cash` tracks raw flows
/// (unaffected by financing) and financing carries no interest
/// compounding — a flat 5% on the peak.
pub fn prepare_cash_budget(req: &CashBudgetRequest) -> CashBudgetResult {
    let mut periods = Vec::with_capacity(12);
    let mut opening = req.opening_cash;
    let mut cumulative = req.opening_cash;
    let mut peak_financing: f64 = 0.0;
    let mut total_receipts = 0.0;
    let mut total_payments = 0.0;

    for i in 1..=12 {
        let quarter = format!("Q{}", (i - 1) / 3 + 1);
        let receipts: f64 = req
            .expected_receipts
            .iter()
            .filter(|r| r.timing.contains(&quarter))
            .map(|r| r.amount)
            .sum();
        let payments: f64 = req
            .expected_payments
            .iter()
            .filter(|p| p.timing.contains(&quarter))
            .map(|p| p.amount)
            .sum();
        let mut closing = opening + receipts - payments;
        let financing;
        if closing < req.minimum_cash_balance {
            financing = req.minimum_cash_balance - closing;
            closing = req.minimum_cash_balance;
        } else {
            financing = 0.0;
        }
        cumulative += receipts - payments;
        if financing > peak_financing {
            peak_financing = financing;
        }
        total_receipts += receipts;
        total_payments += payments;
        periods.push(CashBudgetPeriod {
            period: format!("Month {i}"),
            opening_balance: opening,
            receipts,
            payments,
            closing_balance: closing,
            financing,
            cumulative_cash: cumulative,
        });
        opening = closing;
    }

    CashBudgetResult {
        company_id: req.company_id.clone(),
        budget_period: req.budget_period.clone(),
        periods,
        total_receipts,
        total_payments,
        net_cash_flow: total_receipts - total_payments,
        minimum_cash_balance: req.minimum_cash_balance,
        peak_financing_required: peak_financing,
        total_financing_cost: peak_financing * 0.05,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> CashBudgetRequest {
        CashBudgetRequest {
            company_id: "c1".into(),
            budget_period: "2027".into(),
            opening_cash: 10000.0,
            minimum_cash_balance: 5000.0,
            expected_receipts: vec![CashFlowItem {
                amount: 20000.0,
                timing: "Q1".into(),
            }],
            expected_payments: vec![CashFlowItem {
                amount: 30000.0,
                timing: "Q1".into(),
            }],
        }
    }

    #[test]
    fn twelve_months_with_financing() {
        let r = prepare_cash_budget(&req());
        assert_eq!(r.periods.len(), 12);
        // Month 1: 10000 + 20000 - 30000 = 0 -> below 5000 -> financing 5000
        assert_eq!(r.periods[0].closing_balance, 5000.0);
        assert_eq!(r.periods[0].financing, 5000.0);
        assert_eq!(r.periods[0].cumulative_cash, 0.0);
        // Month 2-3: no flows, opening 5000 stays
        // Python parity: a "Q1" item lands in months 1, 2 AND 3.
        assert_eq!(r.periods[1].opening_balance, 5000.0);
        assert_eq!(r.periods[1].closing_balance, 5000.0);
        assert_eq!(r.periods[1].financing, 10000.0);
        assert_eq!(r.total_receipts, 60000.0);
        assert_eq!(r.total_payments, 90000.0);
        assert_eq!(r.net_cash_flow, -30000.0);
        assert_eq!(r.peak_financing_required, 10000.0);
        assert_eq!(r.total_financing_cost, 500.0);
    }

    #[test]
    fn quarterly_timing_codes() {
        let mut rq = req();
        rq.expected_receipts = vec![CashFlowItem {
            amount: 1000.0,
            timing: "Q4".into(),
        }];
        rq.expected_payments = vec![];
        let r = prepare_cash_budget(&rq);
        // Q4 = months 10, 11, 12
        assert_eq!(r.periods[8].receipts, 0.0);
        assert_eq!(r.periods[9].receipts, 1000.0);
        assert_eq!(r.periods[11].receipts, 1000.0);
    }

    #[test]
    fn no_items_no_financing() {
        let mut rq = req();
        rq.expected_receipts.clear();
        rq.expected_payments.clear();
        let r = prepare_cash_budget(&rq);
        assert_eq!(r.peak_financing_required, 0.0);
        assert_eq!(r.periods[0].closing_balance, 10000.0);
    }
}
