//! retained-profits-service: profit appropriation statements and
//! interim dividend declarations.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppropriationResult {
    pub net_profit: f64,
    pub appropriations: Vec<(String, f64)>,
    pub total_appropriations: f64,
    pub balance_carried_forward: f64,
    pub opening_retained_earnings: f64,
    pub closing_retained_earnings: f64,
}

/// Balance carried forward is net profit less appropriations (may go
/// negative when appropriations exceed profit, matching Python).
pub fn create_appropriation(
    net_profit: f64,
    opening_retained_earnings: f64,
    appropriations: &[(String, f64)],
) -> AppropriationResult {
    let total: f64 = appropriations.iter().map(|(_, v)| *v).sum();
    let balance_carried_forward = net_profit - total;
    AppropriationResult {
        net_profit,
        appropriations: appropriations.to_vec(),
        total_appropriations: total,
        balance_carried_forward,
        opening_retained_earnings,
        closing_retained_earnings: opening_retained_earnings + balance_carried_forward,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DividendResult {
    pub gross_amount: f64,
    pub withholding_tax: f64,
    pub net_payment: f64,
    pub retained_earnings_charge: f64,
}

/// Interim dividend: 15% withholding on the gross, charged against
/// retained earnings at the gross amount.
pub fn declare_interim_dividend(amount: f64) -> DividendResult {
    let withholding_tax = py_round2(amount * 0.15);
    DividendResult {
        gross_amount: amount,
        withholding_tax,
        net_payment: py_round2(amount - withholding_tax),
        retained_earnings_charge: amount,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appropriation_rolls_forward() {
        let r = create_appropriation(
            100_000.0,
            250_000.0,
            &[
                ("dividends".into(), 50_000.0),
                ("general_reserve".into(), 20_000.0),
            ],
        );
        assert_eq!(r.total_appropriations, 70_000.0);
        assert_eq!(r.balance_carried_forward, 30_000.0);
        assert_eq!(r.closing_retained_earnings, 280_000.0);
    }

    #[test]
    fn over_appropriation_goes_negative() {
        let r = create_appropriation(10_000.0, 5_000.0, &[("dividends".into(), 15_000.0)]);
        assert_eq!(r.balance_carried_forward, -5_000.0);
        assert_eq!(r.closing_retained_earnings, 0.0);
    }

    #[test]
    fn interim_dividend_withholding() {
        let d = declare_interim_dividend(20_000.0);
        assert_eq!(d.withholding_tax, 3_000.0);
        assert_eq!(d.net_payment, 17_000.0);
        assert_eq!(d.retained_earnings_charge, 20_000.0);
    }
}
