//! Defined benefit pension: DBO roll-forward, plan assets, and the net interest.

use serde::Serialize;
use serde_json::{json, Value};

use crate::money::money;

#[derive(Debug, Clone, PartialEq)]
pub struct PensionRequest {
    pub company_id: String,
    pub fiscal_year: i32,
    pub plan_name: String,
    pub obligation_beginning: f64,
    pub service_cost: f64,
    pub interest_cost: f64,
    pub benefits_paid: f64,
    pub actuarial_gain_loss: f64,
    pub past_service_cost: f64,
    pub plan_assets_beginning: f64,
    pub contributions: f64,
    pub expected_return: f64,
    pub actual_return: f64,
    pub benefits_paid_from_assets: f64,
}

impl PensionRequest {
    pub fn new(
        company_id: &str,
        fiscal_year: i32,
        plan_name: &str,
        obligation_beginning: f64,
        plan_assets_beginning: f64,
    ) -> PensionRequest {
        PensionRequest {
            company_id: company_id.to_string(),
            fiscal_year,
            plan_name: plan_name.to_string(),
            obligation_beginning,
            service_cost: 0.0,
            interest_cost: 0.0,
            benefits_paid: 0.0,
            actuarial_gain_loss: 0.0,
            past_service_cost: 0.0,
            plan_assets_beginning,
            contributions: 0.0,
            expected_return: 0.0,
            actual_return: 0.0,
            benefits_paid_from_assets: 0.0,
        }
    }

    pub fn with_costs(
        mut self,
        service_cost: f64,
        interest_cost: f64,
        past_service_cost: f64,
    ) -> PensionRequest {
        self.service_cost = service_cost;
        self.interest_cost = interest_cost;
        self.past_service_cost = past_service_cost;
        self
    }

    pub fn with_benefits(mut self, paid_from_plan: f64) -> PensionRequest {
        self.benefits_paid = paid_from_plan;
        self.benefits_paid_from_assets = paid_from_plan;
        self
    }

    pub fn with_actuarial(mut self, gain_loss: f64) -> PensionRequest {
        self.actuarial_gain_loss = gain_loss;
        self
    }

    pub fn with_assets(
        mut self,
        contributions: f64,
        expected_return: f64,
        actual_return: f64,
    ) -> PensionRequest {
        self.contributions = contributions;
        self.expected_return = expected_return;
        self.actual_return = actual_return;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PensionResult {
    pub company_id: String,
    pub plan_name: String,
    pub fiscal_year: i32,
    pub dbo_end: f64,
    pub plan_assets_end: f64,
    pub funded_position: f64,
    pub net_defined_benefit_liability: f64,
    pub current_service_cost: f64,
    pub interest_cost: f64,
    pub net_interest: f64,
    pub remeasurements: f64,
    pub total_pension_expense: f64,
    pub oci_actuarial_gain_loss: f64,
    pub balance_sheet_entry: Value,
}

/// Roll the plan forward one year.
///
/// Preserved quirk: when `actual_return` is zero the expected return stands in
/// for it, so a caller who leaves the actual figure out gets no remeasurement.
/// The funded position is assets minus obligation, and only the deficit becomes
/// a liability, so a surplus never shows as a negative liability.
pub fn calculate(request: &PensionRequest) -> PensionResult {
    let dbo_end = request.obligation_beginning
        + request.service_cost
        + request.interest_cost
        + request.actuarial_gain_loss
        + request.past_service_cost
        - request.benefits_paid;

    let actual_return = if request.actual_return == 0.0 {
        request.expected_return
    } else {
        request.actual_return
    };
    let plan_assets_end = request.plan_assets_beginning + request.contributions + actual_return
        - request.benefits_paid_from_assets;

    let funded = plan_assets_end - dbo_end;
    let net_liability = (-funded).max(0.0);
    let net_asset = funded.max(0.0);
    let net_interest = request.interest_cost - request.expected_return;
    let remeasurements = request.actuarial_gain_loss + (actual_return - request.expected_return);
    let total_expense = request.service_cost + net_interest + request.past_service_cost;

    PensionResult {
        company_id: request.company_id.clone(),
        plan_name: request.plan_name.clone(),
        fiscal_year: request.fiscal_year,
        dbo_end: money(dbo_end),
        plan_assets_end: money(plan_assets_end),
        funded_position: money(funded),
        net_defined_benefit_liability: money(net_liability),
        current_service_cost: money(request.service_cost),
        interest_cost: money(request.interest_cost),
        net_interest: money(net_interest),
        remeasurements: money(remeasurements),
        total_pension_expense: money(total_expense),
        oci_actuarial_gain_loss: money(request.actuarial_gain_loss),
        balance_sheet_entry: json!({
            "defined_benefit_asset": money(net_asset),
            "defined_benefit_liability": money(net_liability),
        }),
    }
}
