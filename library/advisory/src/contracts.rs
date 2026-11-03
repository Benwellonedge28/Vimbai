//! Construction contract advisory: percentage-of-completion revenue and loss
//! provisions. Ported from construction-contracts-service.
//!
//! Kept as shipped: a missing `estimated_total_costs` falls back to costs
//! incurred, so such a contract reads as 100% complete; revenue is value times
//! progress; recognized profit floors at zero when revenue does not cover
//! costs; a loss contract provisions the absolute expected loss.

use crate::{div, num, r2, rows, text};
use serde_json::{json, Value};

pub fn analyze_construction_contracts(request: &Value) -> Value {
    let mut analysed = Vec::new();
    let mut total_value = 0.0;
    let mut total_revenue = 0.0;
    let mut total_provision = 0.0;
    let mut completed = 0i64;
    let mut ongoing = 0i64;

    for contract in rows(request, "contracts") {
        let value = num(contract, "contract_value");
        let costs = num(contract, "costs_incurred");
        let etc = if contract.get("estimated_total_costs").is_some() {
            num(contract, "estimated_total_costs")
        } else {
            costs
        };
        let progress = div(costs, etc);
        let revenue = value * progress;
        let expected_profit = value - etc;
        let provision = if expected_profit < 0.0 {
            expected_profit.abs()
        } else {
            0.0
        };

        total_value += value;
        total_revenue += revenue;
        total_provision += provision;
        if crate::flag(contract, "completed") {
            completed += 1;
        } else {
            ongoing += 1;
        }

        analysed.push(json!({
            "contract_id": text(contract, "id"),
            "contract_value": value,
            "costs_incurred": costs,
            "estimated_total_costs": etc,
            "progress_percentage": r2(progress * 100.0),
            "revenue_to_date": r2(revenue),
            "recognized_profit": r2(if revenue > costs { revenue - costs } else { 0.0 }),
            "expected_profit": r2(expected_profit),
            "provision_for_losses": r2(provision),
        }));
    }

    json!({
        "company_id": text(request, "company_id"),
        "period": text(request, "period"),
        "contract_analysis": analysed,
        "total_contract_value": r2(total_value),
        "total_revenue_recognized": r2(total_revenue),
        "total_provision_for_losses": r2(total_provision),
        "completed_contracts": completed,
        "ongoing_contracts": ongoing,
        "recommendations": [
            "Review cost estimates for contracts with low progress",
            "Recognize losses immediately when identified",
            "Ensure percentage of completion calculations are accurate",
        ],
    })
}
