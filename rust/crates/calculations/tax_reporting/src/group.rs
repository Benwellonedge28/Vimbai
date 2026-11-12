//! Group tax: offsets losses inside the group and reports what consolidation saved.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use vimbai_common::numbers::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subsidiary {
    pub entity_id: String,
    pub entity_name: String,
    pub jurisdiction: String,
    pub pre_tax_income: f64,
    pub tax_paid: f64,
    pub tax_rate: f64,
    pub loss_carryforward: f64,
}

impl Subsidiary {
    pub fn new(
        entity_id: &str,
        entity_name: &str,
        jurisdiction: &str,
        pre_tax_income: f64,
        tax_rate: f64,
    ) -> Subsidiary {
        Subsidiary {
            entity_id: entity_id.to_string(),
            entity_name: entity_name.to_string(),
            jurisdiction: jurisdiction.to_string(),
            pre_tax_income,
            tax_paid: 0.0,
            tax_rate,
            loss_carryforward: 0.0,
        }
    }

    pub fn with_tax_paid(mut self, tax_paid: f64) -> Subsidiary {
        self.tax_paid = tax_paid;
        self
    }

    pub fn with_loss_carryforward(mut self, amount: f64) -> Subsidiary {
        self.loss_carryforward = amount;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupTaxRequest {
    pub group_id: String,
    pub fiscal_year: i32,
    pub subsidiaries: Vec<Subsidiary>,
    pub group_tax_rate: f64,
    pub intercompany_eliminations: f64,
}

impl GroupTaxRequest {
    pub fn new(group_id: &str, fiscal_year: i32, subsidiaries: Vec<Subsidiary>) -> GroupTaxRequest {
        GroupTaxRequest {
            group_id: group_id.to_string(),
            fiscal_year,
            subsidiaries,
            group_tax_rate: 0.25,
            intercompany_eliminations: 0.0,
        }
    }

    pub fn with_group_rate(mut self, rate: f64) -> GroupTaxRequest {
        self.group_tax_rate = rate;
        self
    }

    pub fn with_eliminations(mut self, amount: f64) -> GroupTaxRequest {
        self.intercompany_eliminations = amount;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupTaxResult {
    pub group_id: String,
    pub fiscal_year: i32,
    pub consolidated_income: f64,
    pub consolidated_tax: f64,
    pub total_tax_already_paid: f64,
    pub net_group_tax: f64,
    pub tax_savings_from_consolidation: f64,
    pub subsidiary_summary: Vec<Value>,
}

/// Consolidate group tax.
///
/// Preserved quirk, and worth knowing before anyone "fixes" it: the saving is
/// standalone liability minus consolidated tax minus the *unpaid* standalone
/// shortfall, then clamped at zero. For a group that has already paid its
/// standalone tax in full the shortfall is zero and the figure is the real
/// saving; for a group that has paid nothing, the second term cancels the first
/// and no saving is reported, because nothing has been remitted yet.
pub fn consolidate(request: &GroupTaxRequest) -> GroupTaxResult {
    let total_income: f64 = request.subsidiaries.iter().map(|s| s.pre_tax_income).sum();
    let total_tax_paid: f64 = request.subsidiaries.iter().map(|s| s.tax_paid).sum();

    let consolidated_income = (total_income - request.intercompany_eliminations).max(0.0);
    let consolidated_tax = consolidated_income * request.group_tax_rate;

    let standalone_tax: f64 = request
        .subsidiaries
        .iter()
        .map(|s| s.pre_tax_income.max(0.0) * s.tax_rate)
        .sum();
    let unpaid_standalone: f64 = request
        .subsidiaries
        .iter()
        .filter(|s| s.pre_tax_income > 0.0)
        .map(|s| s.pre_tax_income.max(0.0) * s.tax_rate - s.tax_paid)
        .sum();

    let savings = (standalone_tax - consolidated_tax - unpaid_standalone).max(0.0);

    let subsidiary_summary = request
        .subsidiaries
        .iter()
        .map(|s| {
            json!({
                "entity_id": s.entity_id,
                "name": s.entity_name,
                "jurisdiction": s.jurisdiction,
                "income": s.pre_tax_income,
                "tax_paid": s.tax_paid,
                "rate": s.tax_rate,
                "loss_carryforward": s.loss_carryforward,
            })
        })
        .collect();

    GroupTaxResult {
        group_id: request.group_id.clone(),
        fiscal_year: request.fiscal_year,
        consolidated_income: money(consolidated_income),
        consolidated_tax: money(consolidated_tax),
        total_tax_already_paid: money(total_tax_paid),
        net_group_tax: money((consolidated_tax - total_tax_paid).max(0.0)),
        tax_savings_from_consolidation: money(savings),
        subsidiary_summary,
    }
}
