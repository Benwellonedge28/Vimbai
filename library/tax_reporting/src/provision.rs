//! Current and deferred tax, from temporary differences.

use serde::{Deserialize, Serialize};

use vimbai_common::numbers::{money, round_dp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporaryDifference {
    pub description: String,
    pub book_amount: f64,
    pub tax_amount: f64,
    /// `taxable` or `deductible`.
    pub r#type: String,
}

impl TemporaryDifference {
    pub fn new(
        description: &str,
        book_amount: f64,
        tax_amount: f64,
        kind: &str,
    ) -> TemporaryDifference {
        TemporaryDifference {
            description: description.to_string(),
            book_amount,
            tax_amount,
            r#type: kind.to_string(),
        }
    }

    fn difference(&self) -> f64 {
        self.book_amount - self.tax_amount
    }

    fn is_deductible(&self) -> bool {
        self.r#type == "deductible"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProvisionRequest {
    pub company_id: String,
    pub fiscal_year: i32,
    pub pre_tax_income: f64,
    pub statutory_rate: f64,
    pub permanent_differences: f64,
    pub temp_differences: Vec<TemporaryDifference>,
    pub loss_carryforward: f64,
    pub valuation_allowance: f64,
}

impl ProvisionRequest {
    pub fn new(company_id: &str, fiscal_year: i32, pre_tax_income: f64) -> ProvisionRequest {
        ProvisionRequest {
            company_id: company_id.to_string(),
            fiscal_year,
            pre_tax_income,
            statutory_rate: 0.25,
            permanent_differences: 0.0,
            temp_differences: Vec::new(),
            loss_carryforward: 0.0,
            valuation_allowance: 0.0,
        }
    }

    pub fn with_rate(mut self, statutory_rate: f64) -> ProvisionRequest {
        self.statutory_rate = statutory_rate;
        self
    }

    pub fn with_permanent_differences(mut self, amount: f64) -> ProvisionRequest {
        self.permanent_differences = amount;
        self
    }

    pub fn with_temporary_differences(
        mut self,
        differences: Vec<TemporaryDifference>,
    ) -> ProvisionRequest {
        self.temp_differences = differences;
        self
    }

    pub fn with_loss_carryforward(mut self, amount: f64) -> ProvisionRequest {
        self.loss_carryforward = amount;
        self
    }

    pub fn with_valuation_allowance(mut self, amount: f64) -> ProvisionRequest {
        self.valuation_allowance = amount;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProvisionResult {
    pub company_id: String,
    pub fiscal_year: i32,
    pub current_tax_expense: f64,
    pub deferred_tax_expense: f64,
    pub total_tax_expense: f64,
    pub effective_rate: f64,
    pub deferred_tax_assets: f64,
    pub deferred_tax_liabilities: f64,
    pub net_deferred_tax: f64,
    pub valuation_allowance: f64,
    pub loss_carryforward_utilized: f64,
    pub statutory_rate: f64,
}

/// Compute the provision.
///
/// Preserved: the deferred movement is `dtl - dta`, so a book with larger
/// deductible differences (a net deferred tax asset) reports a *negative*
/// deferred expense. That is the sign convention the posting rules expect.
pub fn calculate(request: &ProvisionRequest) -> ProvisionResult {
    let mut taxable_income = request.pre_tax_income + request.permanent_differences;
    for difference in &request.temp_differences {
        if difference.is_deductible() {
            taxable_income -= difference.difference();
        } else {
            taxable_income += difference.difference();
        }
    }

    let utilized = if request.loss_carryforward > 0.0 {
        request.loss_carryforward.min(taxable_income.max(0.0))
    } else {
        0.0
    };
    taxable_income -= utilized;

    let current_tax = taxable_income.max(0.0) * request.statutory_rate;

    let mut dta = 0.0;
    let mut dtl = 0.0;
    for difference in &request.temp_differences {
        let amount = difference.difference().abs() * request.statutory_rate;
        if difference.is_deductible() {
            dta += amount;
        } else {
            dtl += amount;
        }
    }
    let net_deferred = dta - dtl - request.valuation_allowance;
    let deferred_tax = dtl - dta;
    let total_tax = current_tax + deferred_tax;
    let effective_rate = if request.pre_tax_income != 0.0 {
        total_tax / request.pre_tax_income
    } else {
        0.0
    };

    ProvisionResult {
        company_id: request.company_id.clone(),
        fiscal_year: request.fiscal_year,
        current_tax_expense: money(current_tax),
        deferred_tax_expense: money(deferred_tax),
        total_tax_expense: money(total_tax),
        effective_rate: round_dp(effective_rate, 4),
        deferred_tax_assets: money(dta),
        deferred_tax_liabilities: money(dtl),
        net_deferred_tax: money(net_deferred),
        valuation_allowance: request.valuation_allowance,
        loss_carryforward_utilized: money(utilized),
        statutory_rate: request.statutory_rate,
    }
}
