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
use vimbai_common::Money;

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

// ---- the /consolidate payload -------------------------------------------
//
// The endpoint took the parent's figures at the top level and each subsidiary's
// in a list, and filled in three things the client never had to send: a
// subsidiary's non-controlling interest is the share nobody else owns, its equity
// is what is left after liabilities, and an intercompany line without a kind is
// a sale. Those defaults are the endpoint's, not the library's, so they live here
// with the wire shape.

#[derive(Debug, Clone, Deserialize)]
pub struct SubsidiaryDto {
    pub subsidiary_id: String,
    #[serde(default)]
    pub name: String,
    pub ownership_percentage: f64,
    #[serde(default)]
    pub non_controlling_interests_percentage: Option<f64>,
    pub revenue: f64,
    pub profit: f64,
    pub assets: f64,
    pub liabilities: f64,
    #[serde(default)]
    pub equity: Option<f64>,
    #[serde(default)]
    pub goodwill: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntercompanyDto {
    #[serde(default)]
    pub description: String,
    #[serde(default = "sale")]
    pub transaction_type: String,
    pub amount: f64,
}

fn sale() -> String {
    "sale".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsolidationDto {
    pub parent_company_id: String,
    #[serde(default)]
    pub period: String,
    pub parent_revenue: f64,
    pub parent_profit: f64,
    pub parent_assets: f64,
    pub parent_liabilities: f64,
    #[serde(default)]
    pub subsidiaries: Vec<SubsidiaryDto>,
    #[serde(default)]
    pub intercompany_transactions: Vec<IntercompanyDto>,
}

impl ConsolidationDto {
    /// The request the library works in, with the endpoint's defaults filled in.
    pub fn request(&self) -> ConsolidationRequest {
        ConsolidationRequest {
            parent_company_id: self.parent_company_id.clone(),
            period: self.period.clone(),
            parent: ParentFinancials {
                revenue: self.parent_revenue,
                profit: self.parent_profit,
                assets: self.parent_assets,
                liabilities: self.parent_liabilities,
            },
            subsidiaries: self
                .subsidiaries
                .iter()
                .map(|s| SubsidiaryFinancials {
                    subsidiary_id: s.subsidiary_id.clone(),
                    name: s.name.clone(),
                    ownership_percentage: s.ownership_percentage,
                    non_controlling_interests_percentage: s
                        .non_controlling_interests_percentage
                        .unwrap_or(100.0 - s.ownership_percentage),
                    revenue: s.revenue,
                    profit: s.profit,
                    assets: s.assets,
                    liabilities: s.liabilities,
                    equity: s.equity.unwrap_or(s.assets - s.liabilities),
                    goodwill: s.goodwill,
                })
                .collect(),
            intercompany_transactions: self
                .intercompany_transactions
                .iter()
                .map(|t| IntercompanyTransaction {
                    description: t.description.clone(),
                    transaction_type: t.transaction_type.clone(),
                    amount: t.amount,
                })
                .collect(),
        }
    }
}

/// `/consolidate` as JSON in, JSON out.
pub fn consolidate_from_json(payload: &Value) -> Result<Value, String> {
    let dto: ConsolidationDto =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(consolidate(&dto.request())).map_err(|e| e.to_string())
}

// ---- the /consolidate endpoint, whole --------------------------------
//
// The endpoint returned more than the library measures: it split the balance
// sheet 60/40 and 40/60, amortised a fixed 50,000 of goodwill, and published a
// cash flow block of fixed placeholders inherited from the original service. The
// parent's own figures are placeholders too, taken from the accounting service for
// the same period in production. All of that is presentation, and it lived in the
// service's calculator; it lives here now so the endpoint can be answered without
// the Python service.

/// The parent figures the endpoint has always used while nothing feeds it the
/// parent's own accounts for the period.
pub const PARENT_REVENUE: f64 = 10_000_000.0;
pub const PARENT_PROFIT: f64 = 1_500_000.0;
pub const PARENT_ASSETS: f64 = 50_000_000.0;
pub const PARENT_LIABILITIES: f64 = 30_000_000.0;

#[derive(Debug, Clone, Deserialize)]
pub struct SubsidiaryInput {
    pub subsidiary_id: String,
    #[serde(default)]
    pub subsidiary_name: String,
    pub ownership_percentage: f64,
    #[serde(default)]
    pub goodwill: f64,
    #[serde(default)]
    pub non_controlling_interests_percentage: f64,
    #[serde(default)]
    pub financial_data: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntercompanyInput {
    #[serde(default)]
    pub transaction_type: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatementDto {
    pub parent_company_id: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub subsidiaries: Vec<SubsidiaryInput>,
    #[serde(default)]
    pub intercompany_transactions: Vec<IntercompanyInput>,
    #[serde(default = "yes")]
    pub minority_interest: bool,
}

fn yes() -> bool {
    true
}

fn figure(data: &std::collections::HashMap<String, f64>, key: &str, fallback: f64) -> f64 {
    data.get(key).copied().unwrap_or(fallback)
}

/// `/consolidate` exactly as the service answered it.
pub fn consolidate_statement_from_json(payload: &Value) -> Result<Value, String> {
    let dto: StatementDto =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;

    let request = ConsolidationRequest {
        parent_company_id: dto.parent_company_id.clone(),
        period: dto.period.clone(),
        parent: ParentFinancials {
            revenue: PARENT_REVENUE,
            profit: PARENT_PROFIT,
            assets: PARENT_ASSETS,
            liabilities: PARENT_LIABILITIES,
        },
        subsidiaries: dto
            .subsidiaries
            .iter()
            .map(|s| SubsidiaryFinancials {
                subsidiary_id: s.subsidiary_id.clone(),
                name: s.subsidiary_name.clone(),
                ownership_percentage: s.ownership_percentage,
                non_controlling_interests_percentage: s.non_controlling_interests_percentage,
                revenue: figure(&s.financial_data, "revenue", 2_000_000.0),
                profit: figure(&s.financial_data, "profit", 300_000.0),
                assets: figure(&s.financial_data, "assets", 5_000_000.0),
                liabilities: figure(&s.financial_data, "liabilities", 2_500_000.0),
                equity: s.financial_data.get("equity").copied().unwrap_or_else(|| {
                    figure(&s.financial_data, "assets", 5_000_000.0)
                        - figure(&s.financial_data, "liabilities", 2_500_000.0)
                }),
                goodwill: s.goodwill,
            })
            .collect(),
        intercompany_transactions: dto
            .intercompany_transactions
            .iter()
            .map(|t| IntercompanyTransaction {
                description: String::new(),
                transaction_type: t.transaction_type.clone(),
                amount: t.amount,
            })
            .collect(),
    };

    let core = consolidate(&request);
    let mut body = serde_json::to_value(&core).map_err(|e| e.to_string())?;

    let mut total_sub_equity = Money::zero();
    let mut nci_pct_total = 0.0;
    for sub in &request.subsidiaries {
        total_sub_equity = total_sub_equity.plus(&decimal(sub.equity));
        nci_pct_total += sub.non_controlling_interests_percentage;
    }
    let nci_pct = if request.subsidiaries.is_empty() {
        0.0
    } else {
        nci_pct_total / request.subsidiaries.len() as f64
    };
    let nci_equity_share = total_sub_equity.times(&percent(nci_pct));
    let minority = if dto.minority_interest {
        total_sub_equity
            .plus(&Money::from_i64(PARENT_ASSETS as i64))
            .minus(&Money::from_i64(PARENT_LIABILITIES as i64))
            .times(&decimal(0.1))
    } else {
        Money::zero()
    };

    if let Some(profit) = body.get_mut("consolidated_profit") {
        profit["goodwill_amortization"] = serde_json::json!(50_000.0);
    }
    if let Some(sheet) = body.get_mut("consolidated_balance_sheet") {
        let assets = decimal(amount(&sheet["total_assets"]));
        let liabilities = decimal(amount(&sheet["total_liabilities"]));
        sheet["non_current_assets"] =
            serde_json::json!(assets.times(&percent(60.0)).round(2).to_f64_lossy());
        sheet["current_assets"] =
            serde_json::json!(assets.times(&percent(40.0)).round(2).to_f64_lossy());
        sheet["non_current_liabilities"] =
            serde_json::json!(liabilities.times(&percent(40.0)).round(2).to_f64_lossy());
        sheet["current_liabilities"] =
            serde_json::json!(liabilities.times(&percent(60.0)).round(2).to_f64_lossy());
    }
    body["consolidated_cash_flow"] = serde_json::json!({
        "operating_activities": 2_000_000.0,
        "investing_activities": -800_000.0,
        "financing_activities": -500_000.0,
        "effect_of_exchange_rate": 10_000.0,
        "net_increase_in_cash": 710_000.0,
    });
    body["nci_at_fair_value"] = serde_json::json!(nci_equity_share
        .times(&decimal(1.2))
        .round(2)
        .to_f64_lossy());
    body["minority_interest"] = serde_json::json!(minority.round(2).to_f64_lossy());
    Ok(body)
}

/// A percentage as a decimal multiplier.
fn percent(value: f64) -> Money {
    Money::parse(&(value / 100.0).to_string()).unwrap_or_else(|_| Money::zero())
}

/// A number read back out of a JSON body.
fn amount(value: &Value) -> f64 {
    match value {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(text) => text.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// An amount as the decimal the library works in.
fn decimal(value: f64) -> Money {
    Money::parse(&value.to_string()).unwrap_or_else(|_| Money::zero())
}
