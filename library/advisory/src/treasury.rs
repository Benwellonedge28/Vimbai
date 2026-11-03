//! Treasury advisory endpoints: cash pooling, debt covenants and director
//! emoluments. Ported from cash-pooling-service, debt-covenants-service and
//! director-emoluments-service.
//!
//! Kept as shipped:
//!
//! * Cash pooling sweeps only the non-pooled excess over the threshold, warns
//!   below 80% pooling efficiency, and offers to close accounts only past three
//!   non-pooled ones.
//! * Covenants are the five fixed tests (D/E 2.0, debt/EBITDA 4.0, interest
//!   coverage 2.5, current ratio 1.2, tangible net worth 0.15); any breach is
//!   HIGH risk, more than two sub-10% headrooms is MEDIUM.
//! * Emoluments adds base, bonus, share options, pension and benefits; the
//!   disclosure check compares the list with itself and so always passes.

use crate::{div, flag, num, r2, rows, text};
use serde_json::{json, Value};

pub fn analyze_cash_pooling(request: &Value) -> Value {
    let accounts = rows(request, "accounts");
    let pooled: Vec<&Value> = accounts.iter().filter(|a| flag(a, "is_pooled")).collect();
    let non_pooled: Vec<&Value> = accounts.iter().filter(|a| !flag(a, "is_pooled")).collect();

    let total_pooled: f64 = pooled.iter().map(|a| num(a, "balance")).sum();
    let total_non_pooled: f64 = non_pooled.iter().map(|a| num(a, "balance")).sum();
    let total_cash: f64 = accounts.iter().map(|a| num(a, "balance")).sum();
    let threshold = num(request, "threshold_amount");
    let excess_cash = (total_non_pooled - threshold).max(0.0);

    let pool_summary = json!({
        "total_accounts": accounts.len(),
        "pooled_accounts": pooled.len(),
        "non_pooled_accounts": non_pooled.len(),
        "total_pooled_cash": r2(total_pooled),
        "total_non_pooled_cash": r2(total_non_pooled),
        "total_cash": r2(total_cash),
        "pooling_efficiency": r2(div(total_pooled, total_cash) * 100.0),
    });

    let account_analysis: Vec<Value> = accounts
        .iter()
        .map(|a| {
            json!({
                "account_id": text(a, "account_id"),
                "account_name": text(a, "account_name"),
                "balance": r2(num(a, "balance")),
                "currency": text(a, "currency"),
                "bank": text(a, "bank"),
                "is_pooled": flag(a, "is_pooled"),
                "surplus_deficit": r2(num(a, "balance") - threshold),
            })
        })
        .collect();

    let mut opportunities = Vec::new();
    if excess_cash > 0.0 {
        opportunities.push(json!({
            "type": "sweep_excess",
            "amount": r2(excess_cash),
            "description": format!("Sweep {} to central pool", crate::py_float2(excess_cash)),
        }));
    }
    if non_pooled.len() > 3 {
        opportunities.push(json!({
            "type": "consolidate_accounts",
            "accounts_to_close": non_pooled.len() - 3,
            "description": "Consider closing redundant accounts",
        }));
    }

    let efficiency = r2(div(total_pooled, total_cash) * 100.0);
    let mut recommendations = Vec::new();
    if efficiency < 80.0 {
        recommendations.push("Improve pooling efficiency by including more accounts".to_string());
    }
    if non_pooled.len() > 5 {
        recommendations
            .push("Too many non-pooled accounts - consolidate where possible".to_string());
    }

    json!({
        "company_id": text(request, "company_id"),
        "pool_summary": pool_summary,
        "account_analysis": account_analysis,
        "optimization_opportunities": opportunities,
        "recommendations": recommendations,
    })
}

fn covenant(
    id: &str,
    kind: &str,
    description: &str,
    name: &str,
    actual: f64,
    threshold: f64,
    operator: &str,
) -> Value {
    let (compliant, headroom) = match operator {
        "<=" => (actual <= threshold, threshold - actual),
        ">=" => (actual >= threshold, actual - threshold),
        "<" => (actual < threshold, threshold - actual),
        _ => (actual > threshold, actual - threshold),
    };
    let headroom_pct = if threshold == 0.0 {
        0.0
    } else {
        headroom / threshold * 100.0
    };
    json!({
        "covenant_id": id,
        "covenant_type": kind,
        "description": description,
        "metrics": [{
            "name": name,
            "actual_value": vimbai_common::py_round(actual, 4),
            "threshold": threshold,
            "operator": operator,
            "compliant": compliant,
            "headroom": vimbai_common::py_round(headroom, 4),
            "headroom_percentage": r2(headroom_pct),
        }],
    })
}

pub fn analyze_debt_covenants(request: &Value, analysis_date: &str) -> Value {
    let total_debt = num(request, "total_debt");
    let ebitda = num(request, "ebitda");
    let debt_to_equity = div(total_debt, num(request, "equity"));
    let debt_to_ebitda = div(total_debt, ebitda);
    let interest_coverage = div(ebitda, num(request, "interest_expense"));
    let current_ratio = div(
        num(request, "current_assets"),
        num(request, "current_liabilities"),
    );
    let total_assets = num(request, "total_assets");
    let debt_to_assets = div(total_debt, total_assets);
    let tangible_ratio = div(num(request, "tangible_net_worth"), total_assets);

    let covenants = vec![
        covenant(
            "LEV-001",
            "Leverage",
            "Debt to Equity ratio covenant",
            "Debt/Equity",
            debt_to_equity,
            2.0,
            "<=",
        ),
        covenant(
            "LEV-002",
            "Leverage",
            "Debt to EBITDA ratio covenant",
            "Debt/EBITDA",
            debt_to_ebitda,
            4.0,
            "<=",
        ),
        covenant(
            "COV-001",
            "Coverage",
            "Interest coverage ratio covenant",
            "Interest Coverage",
            interest_coverage,
            2.5,
            ">=",
        ),
        covenant(
            "LIQ-001",
            "Liquidity",
            "Current ratio covenant",
            "Current Ratio",
            current_ratio,
            1.2,
            ">=",
        ),
        covenant(
            "TNW-001",
            "Net Worth",
            "Minimum tangible net worth",
            "TNW/Total Assets",
            tangible_ratio,
            0.15,
            ">=",
        ),
    ];

    // debt_to_assets is computed by the original and never reported.
    let _ = debt_to_assets;

    let mut all_compliant = true;
    let mut warning_count = 0;
    let mut breached = 0;
    for group in &covenants {
        for metric in group["metrics"].as_array().into_iter().flatten() {
            if metric["compliant"] != json!(true) {
                all_compliant = false;
                breached += 1;
            }
            if metric["headroom_percentage"].as_f64().unwrap_or(0.0) < 10.0 {
                warning_count += 1;
            }
        }
    }

    let breach_risk = if breached > 0 {
        "HIGH"
    } else if warning_count > 2 {
        "MEDIUM"
    } else {
        "LOW"
    };

    let mut recommendations = Vec::new();
    if breached > 0 {
        recommendations
            .push("URGENT: Covenant breach detected - engage with lenders immediately".to_string());
    }
    if debt_to_ebitda > 3.5 {
        recommendations
            .push("Approaching leverage limit - consider debt reduction strategies".to_string());
    }
    if interest_coverage < 3.0 {
        recommendations.push(
            "Interest coverage declining - monitor closely and consider refinancing".to_string(),
        );
    }

    json!({
        "company_id": text(request, "company_id"),
        "analysis_date": analysis_date,
        "covenants": covenants,
        "overall_compliance": all_compliant,
        "covenant_count": 5,
        "compliant_count": 5 - breached,
        "breached_count": breached,
        "warning_count": warning_count,
        "breach_risk": breach_risk,
        "recommendations": recommendations,
    })
}

pub fn analyze_director_emoluments(request: &Value) -> Value {
    let directors = rows(request, "directors");
    let mut listed = Vec::new();
    let mut total = 0.0;
    let mut highest: f64 = 0.0;

    for director in directors {
        let base = num(director, "base_salary");
        let bonus = num(director, "bonuses");
        let options = num(director, "share_options_value");
        let pension = num(director, "pension_contributions");
        let benefits = num(director, "benefits");
        let total_dir = base + bonus + options + pension + benefits;
        total += total_dir;
        if total_dir > highest {
            highest = total_dir;
        }
        listed.push(json!({
            "director_id": text(director, "id"),
            "director_name": text(director, "name"),
            "role": text(director, "role"),
            "base_salary": base,
            "bonuses": bonus,
            "share_options_value": options,
            "pension_contributions": pension,
            "benefits": benefits,
            "total_emoluments": total_dir,
        }));
    }

    let avg = div(total, directors.len() as f64);
    let emoluments_ratio = div(total, num(request, "total_staff_costs"));
    let ceo_ratio = div(highest, avg);

    json!({
        "company_id": text(request, "company_id"),
        "period": text(request, "period"),
        "director_emoluments": listed.clone(),
        "total_emoluments": r2(total),
        "highest_paid": r2(highest),
        "average_emoluments": r2(avg),
        "emoluments_to_staff_costs": vimbai_common::py_round(emoluments_ratio, 4),
        "ceo_pay_ratio": r2(ceo_ratio),
        "disclosure_compliant": listed.len() == directors.len(),
        "recommendations": [
            "Ensure all director emoluments are fully disclosed",
            "Include equity-settled share-based payments at fair value",
            "Disclose performance criteria for bonus payments",
            "Consider CEO pay ratio disclosure per regulatory requirements",
        ],
    })
}
