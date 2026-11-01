#![forbid(unsafe_code)]
//! Zero-based budgeting with priority funding, ported from
//! zero-based-budget-service `/prepare`.
//! Mandatory items score 100, discretionary 50; sorted descending and
//! funded while budget remains.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpenseItem {
    pub item_id: String,
    pub description: String,
    pub department: String,
    pub requested_amount: f64,
    #[serde(default)]
    pub justification: String,
    pub mandatory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrioritizedItem {
    pub item_id: String,
    pub description: String,
    pub department: String,
    pub requested_amount: f64,
    pub priority_score: f64,
    pub funded: bool,
    pub funded_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroBasedRequest {
    pub company_id: String,
    pub budget_year: String,
    pub total_available: f64,
    pub expense_items: Vec<ExpenseItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroBasedResult {
    pub company_id: String,
    pub budget_year: String,
    pub total_available: f64,
    pub total_requested: f64,
    pub funded_items: Vec<PrioritizedItem>,
    pub unfunded_items: Vec<PrioritizedItem>,
    /// % of items (not value) funded.
    pub funding_coverage: f64,
}

/// Zero-based budget (zero-based-budget-service `/prepare`).
pub fn prepare_zero_based_budget(req: &ZeroBasedRequest) -> ZeroBasedResult {
    let mut prioritized: Vec<PrioritizedItem> = req
        .expense_items
        .iter()
        .map(|item| PrioritizedItem {
            item_id: item.item_id.clone(),
            description: item.description.clone(),
            department: item.department.clone(),
            requested_amount: item.requested_amount,
            priority_score: if item.mandatory { 100.0 } else { 50.0 },
            funded: false,
            funded_amount: 0.0,
        })
        .collect();
    // Stable descending sort (Python's sort on priority_score, reverse)
    prioritized.sort_by(|a, b| {
        b.priority_score
            .partial_cmp(&a.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut remaining = req.total_available;
    let mut funded = Vec::new();
    let mut unfunded = Vec::new();
    for mut item in prioritized {
        if item.requested_amount <= remaining {
            item.funded = true;
            item.funded_amount = item.requested_amount;
            remaining -= item.requested_amount;
            funded.push(item);
        } else {
            unfunded.push(item);
        }
    }
    let total_items = req.expense_items.len();
    ZeroBasedResult {
        company_id: req.company_id.clone(),
        budget_year: req.budget_year.clone(),
        total_available: req.total_available,
        total_requested: req.expense_items.iter().map(|i| i.requested_amount).sum(),
        funding_coverage: if total_items > 0 {
            funded.len() as f64 / total_items as f64 * 100.0
        } else {
            0.0
        },
        funded_items: funded,
        unfunded_items: unfunded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, amount: f64, mandatory: bool) -> ExpenseItem {
        ExpenseItem {
            item_id: id.into(),
            description: format!("Item {id}"),
            department: "ops".into(),
            requested_amount: amount,
            justification: String::new(),
            mandatory,
        }
    }

    #[test]
    fn mandatory_first() {
        let req = ZeroBasedRequest {
            company_id: "c1".into(),
            budget_year: "2027".into(),
            total_available: 1500.0,
            expense_items: vec![
                item("a", 600.0, false),
                item("b", 800.0, true),
                item("c", 900.0, true),
            ],
        };
        let r = prepare_zero_based_budget(&req);
        // Order: b (800, mandatory), c (900, mandatory), a (600, discretionary).
        // b funded (700 left); c does not fit; a then fits (100 left).
        assert_eq!(r.funded_items[0].item_id, "b");
        assert_eq!(r.funded_items[0].priority_score, 100.0);
        assert_eq!(r.funded_items[1].item_id, "a");
        assert_eq!(r.funded_items[1].priority_score, 50.0);
        assert_eq!(r.funded_items[1].funded_amount, 600.0);
        assert_eq!(r.unfunded_items.len(), 1);
        assert_eq!(r.unfunded_items[0].item_id, "c");
        assert!(!r.unfunded_items[0].funded);
        assert_eq!(r.total_requested, 2300.0);
        // coverage is % of ITEMS funded, not value
        assert_eq!(r.funding_coverage, 2.0 / 3.0 * 100.0);
    }

    #[test]
    fn empty_request() {
        let req = ZeroBasedRequest {
            company_id: "c1".into(),
            budget_year: "2027".into(),
            total_available: 1000.0,
            expense_items: vec![],
        };
        let r = prepare_zero_based_budget(&req);
        assert_eq!(r.funding_coverage, 0.0);
        assert_eq!(r.total_requested, 0.0);
    }
}
