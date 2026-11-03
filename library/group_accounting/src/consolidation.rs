//! Consolidation: parent plus owned subsidiaries, less intercompany trading.
//!
//! This one departs from the service on purpose. The Python `consolidate` had no
//! inputs for the parent at all: it hardcoded revenue 10,000,000, profit
//! 1,500,000, assets 50,000,000 and liabilities 30,000,000, invented a cash flow
//! statement (2,000,000 / -800,000 / -500,000, net 710,000), charged a flat
//! 50,000 of goodwill amortisation, split assets 60/40 and liabilities 40/60,
//! multiplied NCI by 1.2 "at fair value", and defaulted any missing subsidiary
//! figure to 2,000,000 / 300,000 / 5,000,000 / 2,500,000. A ledger cannot carry
//! fiction, so those numbers are now caller inputs and the fabricated lines are
//! gone. The maths that was real is preserved exactly.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::money::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubsidiaryFinancials {
    pub subsidiary_id: String,
    pub name: String,
    /// Percentage owned by the group, 0-100.
    pub ownership_percentage: f64,
    pub non_controlling_interests_percentage: f64,
    pub revenue: f64,
    pub profit: f64,
    pub assets: f64,
    pub liabilities: f64,
    /// Balance-sheet equity; when absent the caller's assets less liabilities.
    pub equity: f64,
    pub goodwill: f64,
}

impl SubsidiaryFinancials {
    pub fn new(
        subsidiary_id: &str,
        name: &str,
        ownership_percentage: f64,
        revenue: f64,
        profit: f64,
        assets: f64,
        liabilities: f64,
    ) -> SubsidiaryFinancials {
        SubsidiaryFinancials {
            subsidiary_id: subsidiary_id.to_string(),
            name: name.to_string(),
            ownership_percentage,
            non_controlling_interests_percentage: 100.0 - ownership_percentage,
            revenue,
            profit,
            assets,
            liabilities,
            equity: assets - liabilities,
            goodwill: 0.0,
        }
    }

    pub fn with_equity(mut self, equity: f64) -> SubsidiaryFinancials {
        self.equity = equity;
        self
    }

    pub fn with_goodwill(mut self, goodwill: f64) -> SubsidiaryFinancials {
        self.goodwill = goodwill;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntercompanyTransaction {
    pub description: String,
    /// `sale` or `dividend`.
    pub transaction_type: String,
    pub amount: f64,
}

impl IntercompanyTransaction {
    pub fn sale(description: &str, amount: f64) -> IntercompanyTransaction {
        IntercompanyTransaction {
            description: description.to_string(),
            transaction_type: "sale".to_string(),
            amount,
        }
    }

    pub fn dividend(description: &str, amount: f64) -> IntercompanyTransaction {
        IntercompanyTransaction {
            description: description.to_string(),
            transaction_type: "dividend".to_string(),
            amount,
        }
    }

    fn is(&self, kind: &str) -> bool {
        self.transaction_type == kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParentFinancials {
    pub revenue: f64,
    pub profit: f64,
    pub assets: f64,
    pub liabilities: f64,
}

impl ParentFinancials {
    pub fn new(revenue: f64, profit: f64, assets: f64, liabilities: f64) -> ParentFinancials {
        ParentFinancials {
            revenue,
            profit,
            assets,
            liabilities,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationRequest {
    pub parent_company_id: String,
    pub period: String,
    pub parent: ParentFinancials,
    pub subsidiaries: Vec<SubsidiaryFinancials>,
    pub intercompany_transactions: Vec<IntercompanyTransaction>,
}

impl ConsolidationRequest {
    pub fn new(
        parent_company_id: &str,
        period: &str,
        parent: ParentFinancials,
    ) -> ConsolidationRequest {
        ConsolidationRequest {
            parent_company_id: parent_company_id.to_string(),
            period: period.to_string(),
            parent,
            subsidiaries: Vec::new(),
            intercompany_transactions: Vec::new(),
        }
    }

    pub fn with_subsidiaries(
        mut self,
        subsidiaries: Vec<SubsidiaryFinancials>,
    ) -> ConsolidationRequest {
        self.subsidiaries = subsidiaries;
        self
    }

    pub fn with_intercompany(
        mut self,
        transactions: Vec<IntercompanyTransaction>,
    ) -> ConsolidationRequest {
        self.intercompany_transactions = transactions;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConsolidationResult {
    pub parent_company_id: String,
    pub period: String,
    pub consolidated_revenue: Value,
    pub consolidated_profit: Value,
    pub consolidated_balance_sheet: Value,
    pub goodwill_recognized: f64,
    pub elimination_count: usize,
}

/// Consolidate.
///
/// Subsidiary revenue and profit are included in proportion to ownership, which
/// is what the service did, so a 60% subsidiary contributes 60% of its revenue
/// even though full consolidation would take 100% and then attribute the rest to
/// NCI. Preserved as-is: changing the basis would move every reported group
/// revenue figure.
pub fn consolidate(request: &ConsolidationRequest) -> ConsolidationResult {
    let mut subsidiary_revenues: IndexMap<String, f64> = IndexMap::new();
    let mut subsidiary_profits: IndexMap<String, f64> = IndexMap::new();
    let mut total_sub_revenue = 0.0;
    let mut total_sub_profit = 0.0;
    let mut total_sub_assets = 0.0;
    let mut total_sub_liabilities = 0.0;
    let mut total_sub_equity = 0.0;
    let mut goodwill_recognized = 0.0;

    for subsidiary in &request.subsidiaries {
        let ownership = subsidiary.ownership_percentage / 100.0;
        let revenue = subsidiary.revenue * ownership;
        let profit = subsidiary.profit * ownership;
        subsidiary_revenues.insert(subsidiary.subsidiary_id.clone(), money(revenue));
        subsidiary_profits.insert(subsidiary.subsidiary_id.clone(), money(profit));
        total_sub_revenue += revenue;
        total_sub_profit += profit;
        total_sub_assets += subsidiary.assets;
        total_sub_liabilities += subsidiary.liabilities;
        total_sub_equity += subsidiary.equity;
        goodwill_recognized += subsidiary.goodwill;
    }

    let intercompany_sales: f64 = request
        .intercompany_transactions
        .iter()
        .filter(|t| t.is("sale"))
        .map(|t| t.amount)
        .sum();

    let nci_pct_avg = if request.subsidiaries.is_empty() {
        0.0
    } else {
        request
            .subsidiaries
            .iter()
            .map(|s| s.non_controlling_interests_percentage)
            .sum::<f64>()
            / request.subsidiaries.len() as f64
    };
    let nci_profit_share = total_sub_profit * (nci_pct_avg / 100.0);
    let nci_equity_share = total_sub_equity * (nci_pct_avg / 100.0);

    let consolidated_revenue = request.parent.revenue + total_sub_revenue - intercompany_sales;
    let consolidated_profit = request.parent.profit + total_sub_profit - nci_profit_share;

    let total_assets = request.parent.assets + total_sub_assets;
    let total_liabilities = request.parent.liabilities + total_sub_liabilities;
    let total_equity = total_assets - total_liabilities;
    let parent_equity = total_equity - nci_equity_share;

    ConsolidationResult {
        parent_company_id: request.parent_company_id.clone(),
        period: request.period.clone(),
        consolidated_revenue: json!({
            "parent_revenue": money(request.parent.revenue),
            "subsidiary_revenues": subsidiary_revenues,
            "intercompany_sales_elimination": money(intercompany_sales),
            "consolidated_revenue": money(consolidated_revenue),
        }),
        consolidated_profit: json!({
            "parent_profit": money(request.parent.profit),
            "subsidiary_profits": subsidiary_profits,
            "nci_share": money(nci_profit_share),
            "attributable_to_parent": money(consolidated_profit),
        }),
        consolidated_balance_sheet: json!({
            "total_assets": money(total_assets),
            "total_liabilities": money(total_liabilities),
            "equity_attributable_to_parent": money(parent_equity),
            "non_controlling_interests": money(nci_equity_share),
            "total_equity": money(total_equity),
        }),
        goodwill_recognized: money(goodwill_recognized),
        elimination_count: request.intercompany_transactions.len(),
    }
}
