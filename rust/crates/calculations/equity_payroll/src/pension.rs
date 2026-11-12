//! pension-accounting-service `/calculate`: IAS 19 defined benefit
//! movement, funded position and net pension expense.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PensionRequest {
    pub company_id: String,
    pub plan_name: String,
    pub fiscal_year: i64,
    pub obligation_beginning: f64,
    pub service_cost: f64,
    pub interest_cost: f64,
    pub actuarial_gain_loss: f64,
    pub past_service_cost: f64,
    pub benefits_paid: f64,
    pub plan_assets_beginning: f64,
    pub contributions: f64,
    pub expected_return: f64,
    #[serde(default)]
    pub actual_return: f64,
    pub benefits_paid_from_assets: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PensionResult {
    pub company_id: String,
    pub plan_name: String,
    pub fiscal_year: i64,
    pub dbo_end: f64,
    pub plan_assets_end: f64,
    pub funded_position: f64,
    pub net_defined_benefit_liability: f64,
    pub defined_benefit_asset: f64,
    pub current_service_cost: f64,
    pub interest_cost: f64,
    pub net_interest: f64,
    pub remeasurements: f64,
    pub total_pension_expense: f64,
    pub oci_actuarial_gain_loss: f64,
}

pub fn calculate_pension(req: &PensionRequest) -> PensionResult {
    let dbo_end = req.obligation_beginning
        + req.service_cost
        + req.interest_cost
        + req.actuarial_gain_loss
        + req.past_service_cost
        - req.benefits_paid;
    // Python: `req.actual_return or req.expected_return` -> 0.0 is falsy
    let actual_return = if req.actual_return != 0.0 {
        req.actual_return
    } else {
        req.expected_return
    };
    let plan_assets_end = req.plan_assets_beginning + req.contributions + actual_return
        - req.benefits_paid_from_assets;
    let funded = plan_assets_end - dbo_end;
    let net_liability = (-funded).max(0.0);
    let net_asset = funded.max(0.0);
    let net_interest = req.interest_cost - req.expected_return;
    let remeasurements = req.actuarial_gain_loss + (actual_return - req.expected_return);
    let total_expense = req.service_cost + net_interest + req.past_service_cost;

    PensionResult {
        company_id: req.company_id.clone(),
        plan_name: req.plan_name.clone(),
        fiscal_year: req.fiscal_year,
        dbo_end: py_round2(dbo_end),
        plan_assets_end: py_round2(plan_assets_end),
        funded_position: py_round2(funded),
        net_defined_benefit_liability: py_round2(net_liability),
        defined_benefit_asset: py_round2(net_asset),
        current_service_cost: py_round2(req.service_cost),
        interest_cost: py_round2(req.interest_cost),
        net_interest: py_round2(net_interest),
        remeasurements: py_round2(remeasurements),
        total_pension_expense: py_round2(total_expense),
        oci_actuarial_gain_loss: py_round2(req.actuarial_gain_loss),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> PensionRequest {
        PensionRequest {
            company_id: "c1".into(),
            plan_name: "Main".into(),
            fiscal_year: 2026,
            obligation_beginning: 500_000.0,
            service_cost: 40_000.0,
            interest_cost: 35_000.0,
            actuarial_gain_loss: 10_000.0,
            past_service_cost: 5_000.0,
            benefits_paid: 20_000.0,
            plan_assets_beginning: 450_000.0,
            contributions: 45_000.0,
            expected_return: 30_000.0,
            actual_return: 25_000.0,
            benefits_paid_from_assets: 20_000.0,
        }
    }

    #[test]
    fn dbo_and_assets_movement() {
        let r = calculate_pension(&req());
        // dbo = 500+40+35+10+5-20 = 570k; assets = 450+45+25-20 = 500k
        assert_eq!(r.dbo_end, 570_000.0);
        assert_eq!(r.plan_assets_end, 500_000.0);
        assert_eq!(r.funded_position, -70_000.0);
        assert_eq!(r.net_defined_benefit_liability, 70_000.0);
        assert_eq!(r.defined_benefit_asset, 0.0);
    }

    #[test]
    fn expense_and_remeasurements() {
        let r = calculate_pension(&req());
        // net interest = 35-30 = 5k; expense = 40+5+5 = 50k
        assert_eq!(r.net_interest, 5_000.0);
        assert_eq!(r.total_pension_expense, 50_000.0);
        // remeasurements = 10k + (25k-30k) = 5k
        assert_eq!(r.remeasurements, 5_000.0);
    }

    #[test]
    fn zero_actual_return_falls_back_to_expected() {
        let mut q = req();
        q.actual_return = 0.0;
        let r = calculate_pension(&q);
        // assets = 450+45+30-20 = 505k vs dbo 570k -> still a 65k liability
        assert_eq!(r.plan_assets_end, 505_000.0);
        assert_eq!(r.defined_benefit_asset, 0.0);
        assert_eq!(r.net_defined_benefit_liability, 65_000.0);
    }

    #[test]
    fn surplus_plan_reports_asset_only() {
        let mut q = req();
        q.plan_assets_beginning = 600_000.0;
        let r = calculate_pension(&q);
        assert!(r.defined_benefit_asset > 0.0);
        assert_eq!(r.net_defined_benefit_liability, 0.0);
    }
}
