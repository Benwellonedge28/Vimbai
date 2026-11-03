//! Ledger study endpoints: the analysis half of accounts payable, accounts
//! receivable, general ledger, journal entries and the financial analytics
//! dashboard. Ported from accounts-payable-service, accounts-receivable-service,
//! general-ledger-service, journal-entries-service and analytics-service.
//!
//! None of the five held a record anywhere: each was a single `/analyze` over
//! numbers the caller already had, so they fold into this kernel module instead
//! of running a container apiece.
//!
//! Bodies are taken as `serde_json::Value` and answers built the same way,
//! because the originals returned loose `Dict[str, Any]` shapes with numbers
//! where booleans belonged.
//!
//! Kept as shipped, including the quirks:
//!
//! * Accounts payable ages cumulatively — `0-30` is everything inside 30 days,
//!   `31-60` is everything inside 60, and so on — then books the difference
//!   against total payable *including tax*, so "Over 90 days" goes negative
//!   whenever the earlier buckets overlap. Buckets count invoice amount only.
//! * Accounts receivable reports a hardcoded 45-day DSO and a hardcoded 30 days
//!   overdue on every collection action; both ignore the real invoice dates.
//! * Journal entries compare total debits with total credits computed from the
//!   same column, so the validation can never fail.
//! * Analytics reports gross margin as the net margin, and its benchmark flags
//!   arrive as 1.0/0.0 rather than true/false.

use serde_json::{json, Map, Value};

/// Python's `round()`: half to even, at `scale` decimal places.
pub fn py_round(value: f64, scale: i32) -> f64 {
    let factor = 10f64.powi(scale);
    let scaled = value * factor;
    let floor = scaled.floor();
    let diff = scaled - floor;
    let rounded = if (diff - 0.5).abs() < 1e-9 {
        if (floor as i64) % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    } else {
        scaled.round()
    };
    rounded / factor
}

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn rows<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Python's `str(float)`: whole numbers keep one decimal (`73.0`).
fn py_float_str(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

fn div(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

fn pct(part: f64, whole: f64) -> f64 {
    if whole == 0.0 {
        0.0
    } else {
        r2(part / whole * 100.0)
    }
}

/// Days from `date` to `today`, as a civil-date difference. Anything that does
/// not parse as `YYYY-MM-DD` returns `None`, matching the original's bare
/// `except: return False`.
pub fn days_between(date: &str, today: &str) -> Option<i64> {
    Some(days_from_civil(today)? - days_from_civil(date)?)
}

fn days_from_civil(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe)
}

fn value_map(pairs: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, v) in pairs {
        map.insert(key.to_string(), v);
    }
    Value::Object(map)
}

// ------------------------------------------------------- accounts payable

/// `POST /analyze`. `today` is the ageing reference date (`YYYY-MM-DD`).
pub fn analyze_accounts_payable(request: &Value, today: &str) -> Value {
    let invoices = rows(request, "invoices");

    let total_outstanding: f64 = invoices
        .iter()
        .map(|i| num(i, "amount") + num(i, "tax_amount"))
        .sum();
    let current_count = invoices
        .iter()
        .filter(|i| text(i, "status") == "Current")
        .count();
    let overdue_count = invoices
        .iter()
        .filter(|i| text(i, "status") == "Overdue")
        .count();

    let within = |days: i64| -> f64 {
        invoices
            .iter()
            .filter(|i| match days_between(&text(i, "due_date"), today) {
                Some(elapsed) => elapsed <= days,
                None => false,
            })
            .map(|i| num(i, "amount"))
            .sum()
    };
    let aging_0_30 = within(30);
    let aging_31_60 = within(60);
    let aging_61_90 = within(90);
    let aging_over_90 = total_outstanding - aging_0_30 - aging_31_60 - aging_61_90;

    let bucket = |name: &str, amount: f64| json!({"bucket": name, "amount": r2(amount), "percentage": pct(amount, total_outstanding)});
    let aging_analysis = vec![
        bucket("0-30 days", aging_0_30),
        bucket("31-60 days", aging_31_60),
        bucket("61-90 days", aging_61_90),
        bucket("Over 90 days", aging_over_90),
    ];

    // Python sorted() on the ISO date string, stable on ties.
    let mut order: Vec<usize> = (0..invoices.len()).collect();
    order.sort_by(|&a, &b| text(&invoices[a], "due_date").cmp(&text(&invoices[b], "due_date")));

    let mut remaining_cash = num(request, "available_cash");
    let mut payment_schedule = Vec::new();
    for index in order {
        let invoice = &invoices[index];
        let amount = num(invoice, "amount");
        let paid = remaining_cash >= amount;
        if paid {
            remaining_cash -= amount;
        }
        payment_schedule.push(json!({
            "invoice_id": text(invoice, "invoice_id"),
            "vendor_name": text(invoice, "vendor_name"),
            "amount": amount,
            "due_date": text(invoice, "due_date"),
            "status": if paid { "Scheduled for Payment" } else { "Pending Cash" },
            "priority": if paid { "Normal" } else { "High" },
        }));
    }

    let credit_limit = num(request, "credit_limit");
    let shortfall = (total_outstanding - num(request, "available_cash")).max(0.0);
    let cash_requirements = json!({
        "total_payables": r2(total_outstanding),
        "available_cash": num(request, "available_cash"),
        "cash_shortfall": r2(shortfall),
        "credit_utilization": if credit_limit == 0.0 { 0.0 } else { r2(total_outstanding / credit_limit * 100.0) },
    });

    let mut recommendations = Vec::new();
    if overdue_count > 0 {
        recommendations.push(format!(
            "{overdue_count} overdue invoices require immediate attention"
        ));
    }
    if shortfall > 0.0 {
        recommendations.push("Consider using credit facility to cover cash shortfall".to_string());
    }
    if aging_over_90 > total_outstanding * 0.1 {
        recommendations.push("High overdue amounts - review collection process".to_string());
    }

    json!({
        "company_id": text(request, "company_id"),
        "invoice_summary": {
            "total_invoices": invoices.len(),
            "total_outstanding": r2(total_outstanding),
            "current_invoices": current_count,
            "overdue_invoices": overdue_count,
        },
        "aging_analysis": aging_analysis,
        "payment_schedule": payment_schedule,
        "cash_requirements": cash_requirements,
        "recommendations": recommendations,
    })
}

// ---------------------------------------------------- accounts receivable

pub fn analyze_accounts_receivable(request: &Value) -> Value {
    let invoices = rows(request, "invoices");
    let outstanding = |i: &Value| num(i, "amount") - num(i, "amount_paid");

    let total_ar: f64 = invoices.iter().map(outstanding).sum();
    let by_status = |status: &str| -> f64 {
        invoices
            .iter()
            .filter(|i| text(i, "status") == status)
            .map(outstanding)
            .sum()
    };
    let current_ar = by_status("Current");
    let overdue_30 = by_status("Overdue 30");
    let overdue_60 = by_status("Overdue 60");
    let overdue_90 = by_status("Overdue 90");

    let bucket = |name: &str, amount: f64| json!({"bucket": name, "amount": r2(amount), "percentage": pct(amount, total_ar)});
    let aging_schedule = vec![
        bucket("Current", current_ar),
        bucket("1-30 Days Overdue", overdue_30),
        bucket("31-60 Days Overdue", overdue_60),
        bucket("61-90 Days Overdue", overdue_90),
    ];

    // HARDCODED: the collection period never looks at the invoice dates.
    let avg_collection_period = 45.0;
    let target_dso = num(request, "target_dso");
    let dso_analysis = json!({
        "actual_dso": avg_collection_period,
        "target_dso": target_dso,
        "dso_variance": avg_collection_period - target_dso,
        "collection_efficiency": if target_dso == 0.0 {
            0.0
        } else {
            r2((1.0 - avg_collection_period / target_dso) * 100.0)
        },
    });

    let collection_plan: Vec<Value> = invoices
        .iter()
        .filter(|i| matches!(text(i, "status").as_str(), "Overdue 60" | "Overdue 90"))
        .take(10)
        .map(|i| {
            json!({
                "customer_id": text(i, "customer_id"),
                "customer_name": text(i, "customer_name"),
                "outstanding_amount": r2(outstanding(i)),
                "days_overdue": 30,
                "priority": "High",
                "recommended_action": "Escalate to legal",
            })
        })
        .collect();

    let current_arrears = overdue_30 * 0.05 + overdue_60 * 0.25 + overdue_90 * 0.75;
    let bad_debt_provision = json!({
        "current_arrears": current_arrears,
        "provision_percentage": if total_ar == 0.0 {
            0.0
        } else {
            r2((overdue_30 * 5.0 + overdue_60 * 25.0 + overdue_90 * 75.0) / total_ar * 100.0)
        },
    });

    let mut recommendations = Vec::new();
    if avg_collection_period > target_dso * 1.2 {
        recommendations
            .push("DSO significantly above target - improve collection efforts".to_string());
    }
    if overdue_90 > total_ar * 0.15 {
        recommendations.push("High 90+ day receivables - escalate collection".to_string());
    }
    if current_arrears > total_ar * 0.1 {
        recommendations.push("High bad debt risk - increase provisions".to_string());
    }

    json!({
        "company_id": text(request, "company_id"),
        "ar_summary": {"total_ar": r2(total_ar), "invoice_count": invoices.len()},
        "aging_schedule": aging_schedule,
        "dso_analysis": dso_analysis,
        "collection_plan": collection_plan,
        "bad_debt_provision": bad_debt_provision,
        "recommendations": recommendations,
    })
}

// ---------------------------------------------------------- general ledger

pub fn analyze_general_ledger(request: &Value) -> Value {
    let accounts = rows(request, "accounts");
    let side = |wanted: &str| -> f64 {
        accounts
            .iter()
            .filter(|a| text(a, "debit_credit") == wanted)
            .map(|a| num(a, "balance"))
            .sum()
    };
    let total_debits = side("Debit");
    let total_credits = side("Credit");

    let by_type = |wanted: &str| -> f64 {
        accounts
            .iter()
            .filter(|a| text(a, "account_type") == wanted)
            .map(|a| num(a, "balance"))
            .sum()
    };
    let total_assets = by_type("Asset");
    let total_liabilities = by_type("Liability");
    let total_equity = by_type("Equity");
    let total_revenue = by_type("Revenue");
    let total_expenses = by_type("Expense");

    let variance = (total_debits - total_credits).abs();
    let trial_balance = json!({
        "total_debits": r2(total_debits),
        "total_credits": r2(total_credits),
        "balanced": variance < 0.01,
        "variance": r2(variance),
    });

    let account_analysis: Vec<Value> = accounts
        .iter()
        .map(|a| {
            json!({
                "account_id": text(a, "account_id"),
                "name": text(a, "account_name"),
                "type": text(a, "account_type"),
                "balance": r2(num(a, "balance")),
            })
        })
        .collect();

    let mut recommendations = Vec::new();
    if variance >= 0.01 {
        recommendations.push("Trial balance is not balanced - review entries".to_string());
    }
    if total_revenue < total_expenses {
        recommendations.push("Net loss detected - review expense controls".to_string());
    }

    json!({
        "company_id": text(request, "company_id"),
        "trial_balance": trial_balance,
        "account_analysis": account_analysis,
        "balance_sheet_totals": {
            "total_assets": r2(total_assets),
            "total_liabilities": r2(total_liabilities),
            "total_equity": r2(total_equity),
        },
        "income_statement_totals": {
            "total_revenue": r2(total_revenue),
            "total_expenses": r2(total_expenses),
        },
        "recommendations": recommendations,
    })
}

// --------------------------------------------------------- journal entries

pub fn analyze_journal_entries(request: &Value) -> Value {
    let entries = rows(request, "entries");
    let total_entries = entries.len();
    let posted = entries
        .iter()
        .filter(|e| text(e, "status") == "Posted")
        .count();
    let rejected = entries
        .iter()
        .filter(|e| text(e, "status") == "Rejected")
        .count();

    let pending: Vec<&Value> = entries
        .iter()
        .filter(|e| text(e, "status") == "Pending")
        .collect();

    // QUIRK: both sides sum the same column, so this can never disagree.
    let total_debits: f64 = entries.iter().map(|e| num(e, "amount")).sum();
    let total_credits = total_debits;

    let count_with = |needle: &str| {
        entries
            .iter()
            .filter(|e| text(e, "description").contains(needle))
            .count()
    };

    let pending_entries: Vec<Value> = pending
        .iter()
        .take(10)
        .map(|e| {
            json!({
                "entry_id": text(e, "entry_id"),
                "description": text(e, "description"),
                "amount": num(e, "amount"),
                "date": text(e, "entry_date"),
            })
        })
        .collect();

    let mut recommendations = Vec::new();
    if pending.len() > 20 {
        recommendations.push(format!(
            "{} pending entries - expedite posting",
            pending.len()
        ));
    }
    if (rejected as f64) > total_entries as f64 * 0.05 {
        recommendations.push("High rejection rate - review entry procedures".to_string());
    }

    let variance = (total_debits - total_credits).abs();

    json!({
        "company_id": text(request, "company_id"),
        "entries_summary": {
            "total_entries": total_entries,
            "posted": posted,
            "pending": pending.len(),
            "rejected": rejected,
            "posting_rate": if total_entries == 0 { 0.0 } else { r2(posted as f64 / total_entries as f64 * 100.0) },
        },
        "entries_by_type": {
            "standard_entries": count_with("Standard"),
            "adjusting_entries": count_with("Adjusting"),
            "closing_entries": count_with("Closing"),
            "reversing_entries": count_with("Reversing"),
        },
        "pending_entries": pending_entries,
        "validation_results": {
            "debits_equals_credits": variance < 0.01,
            "total_debits": r2(total_debits),
            "total_credits": r2(total_credits),
            "variance": r2(variance),
        },
        "recommendations": recommendations,
    })
}

// --------------------------------------------------------------- analytics

pub fn analyze_financials(request: &Value) -> Value {
    let f = request
        .get("financials")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let benchmark_in = request
        .get("benchmark")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let revenue = num(&f, "revenue");
    let net_income = num(&f, "net_income");
    let total_assets = num(&f, "total_assets");
    let total_equity = num(&f, "total_equity");
    let total_liabilities = num(&f, "total_liabilities");
    let current_assets = num(&f, "current_assets");
    let current_liabilities = num(&f, "current_liabilities");
    let inventory = num(&f, "inventory");
    let receivables = num(&f, "accounts_receivable");
    let cogs = num(&f, "cogs");
    let operating_cash_flow = num(&f, "operating_cash_flow");

    let gross_margin = r2(div(net_income, revenue) * 100.0);
    let net_margin = r2(div(net_income, revenue) * 100.0);
    let roe = r2(div(net_income, total_equity) * 100.0);
    let roa = r2(div(net_income, total_assets) * 100.0);

    let current_ratio = r2(div(current_assets, current_liabilities));
    let quick_ratio = r2(div(current_assets - inventory, current_liabilities));
    let cash_ratio = r2(div(operating_cash_flow, current_liabilities));

    let debt_to_equity = r2(div(total_liabilities, total_equity));
    let debt_to_assets = r2(div(total_liabilities, total_assets) * 100.0);
    let interest_coverage = if total_liabilities == 0.0 {
        0.0
    } else {
        r2(net_income / (total_liabilities * 0.05).max(1.0))
    };

    let asset_turnover = r2(div(revenue, total_assets));
    let inventory_turnover = r2(div(cogs, inventory));
    let receivables_turnover = r2(div(revenue, receivables));
    let dso = if revenue == 0.0 {
        0.0
    } else {
        py_round(receivables / revenue * 365.0, 1)
    };

    let industry_roe = num(&benchmark_in, "industry_avg_roe") * 100.0;
    let industry_current = num(&benchmark_in, "industry_avg_current_ratio");
    let industry_de = num(&benchmark_in, "industry_avg_debt_to_equity");
    let industry_margin = num(&benchmark_in, "industry_avg_net_margin") * 100.0;

    // QUIRK: the flags are floats, not booleans, exactly as the service
    // serialised its Dict[str, Any].
    let flag = |ok: bool| if ok { 1.0 } else { 0.0 };
    let roe_flag = roe > industry_roe;
    let current_flag = current_ratio > industry_current;
    let de_flag = debt_to_equity < industry_de;
    let margin_flag = net_margin > industry_margin;

    let benchmark = value_map(vec![
        (
            "roe",
            json!({"company": roe, "industry": industry_roe, "above_industry": flag(roe_flag)}),
        ),
        (
            "current_ratio",
            json!({"company": current_ratio, "industry": industry_current, "above_industry": flag(current_flag)}),
        ),
        (
            "debt_to_equity",
            json!({"company": debt_to_equity, "industry": industry_de, "above_industry": flag(de_flag)}),
        ),
        (
            "net_margin",
            json!({"company": net_margin, "industry": industry_margin, "above_industry": flag(margin_flag)}),
        ),
    ]);

    let hits = [roe_flag, current_flag, de_flag, margin_flag]
        .iter()
        .filter(|ok| **ok)
        .count();
    let overall_score = py_round((hits * 25) as f64, 1);

    let mut insights = Vec::new();
    if roe < industry_roe {
        insights.push(
            "ROE below industry average - consider equity efficiency improvements".to_string(),
        );
    }
    if current_ratio < 1.0 {
        insights.push("Current ratio below 1.0 - potential short-term liquidity risk".to_string());
    }
    if debt_to_equity > 1.0 {
        insights.push("High leverage - debt-to-equity ratio above 1.0".to_string());
    }
    if dso > 60.0 {
        insights.push(format!(
            "DSO of {} days suggests collection process needs improvement",
            py_float_str(dso)
        ));
    }
    if insights.is_empty() {
        insights.push("All key metrics within healthy ranges".to_string());
    }

    json!({
        "company_id": text(request, "company_id"),
        "period": text(request, "period"),
        "profitability": {
            "gross_margin": gross_margin,
            "net_margin": net_margin,
            "roe": roe,
            "roa": roa,
        },
        "liquidity": {
            "current_ratio": current_ratio,
            "quick_ratio": quick_ratio,
            "cash_ratio": cash_ratio,
        },
        "solvency": {
            "debt_to_equity": debt_to_equity,
            "debt_to_assets": debt_to_assets,
            "interest_coverage": interest_coverage,
        },
        "efficiency": {
            "asset_turnover": asset_turnover,
            "inventory_turnover": inventory_turnover,
            "receivables_turnover": receivables_turnover,
            "dso": dso,
        },
        "benchmark_comparison": benchmark,
        "overall_score": overall_score,
        "insights": insights,
    })
}

/// The Python services stamped a uuid on their answers. Kept out of the maths so
/// the caller can supply its own, but provided here for wire compatibility.
pub fn with_id(mut answer: Value, id: &str) -> Value {
    if let Value::Object(ref mut map) = answer {
        map.insert("id".to_string(), Value::String(id.to_string()));
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Python and Rust agree on arithmetic, not on float widening: 0 and 0.0
    /// must compare equal, so numbers are normalised before comparing.
    fn normalise(value: &Value) -> Value {
        match value {
            Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
            Value::Array(items) => Value::Array(items.iter().map(normalise).collect()),
            Value::Object(map) => {
                Value::Object(map.iter().map(|(k, v)| (k.clone(), normalise(v))).collect())
            }
            other => other.clone(),
        }
    }

    fn assert_parity(actual: Value, expected: &str) {
        let expected: Value = serde_json::from_str(expected).expect("expected json");
        assert_eq!(
            serde_json::to_string(&normalise(&actual)).unwrap(),
            serde_json::to_string(&normalise(&expected)).unwrap()
        );
    }

    /// 2026-11-03, with due dates placed at 10, 45, 75 and 150 days ago.
    fn payable_request() -> Value {
        json!({
            "company_id": "C1",
            "payment_batch_size": 10,
            "early_payment_discount_rate": 0.02,
            "available_cash": 250,
            "credit_limit": 2000,
            "invoices": [
                {"invoice_id": "I1", "vendor_id": "V1", "vendor_name": "Alpha", "invoice_number": "N1",
                 "invoice_date": "2026-09-24", "due_date": "2026-10-24", "amount": 100, "tax_amount": 15,
                 "status": "Current", "payment_terms": "30d", "line_items": []},
                {"invoice_id": "I2", "vendor_id": "V2", "vendor_name": "Beta", "invoice_number": "N2",
                 "invoice_date": "2026-08-05", "due_date": "2026-09-19", "amount": 200, "tax_amount": 0,
                 "status": "Overdue", "payment_terms": "30d", "line_items": []},
                {"invoice_id": "I3", "vendor_id": "V3", "vendor_name": "Gamma", "invoice_number": "N3",
                 "invoice_date": "2026-07-06", "due_date": "2026-08-20", "amount": 50, "tax_amount": 5,
                 "status": "Overdue", "payment_terms": "60d", "line_items": []},
                {"invoice_id": "I4", "vendor_id": "V4", "vendor_name": "Delta", "invoice_number": "N4",
                 "invoice_date": "2026-04-17", "due_date": "2026-06-06", "amount": 80, "tax_amount": 0,
                 "status": "Overdue", "payment_terms": "30d", "line_items": []}
            ]
        })
    }

    #[test]
    fn accounts_payable_matches_the_service() {
        // Captured from accounts-payable-service on 2026-11-03. The negative
        // "Over 90 days" figure is the original's cumulative ageing.
        let expected = r#"{
            "company_id": "C1",
            "invoice_summary": {"total_invoices": 4, "total_outstanding": 450.0,
                                "current_invoices": 1, "overdue_invoices": 3},
            "aging_analysis": [
                {"bucket": "0-30 days", "amount": 100.0, "percentage": 22.22},
                {"bucket": "31-60 days", "amount": 300.0, "percentage": 66.67},
                {"bucket": "61-90 days", "amount": 350.0, "percentage": 77.78},
                {"bucket": "Over 90 days", "amount": -300.0, "percentage": -66.67}
            ],
            "payment_schedule": [
                {"invoice_id": "I4", "vendor_name": "Delta", "amount": 80.0, "due_date": "2026-06-06",
                 "status": "Scheduled for Payment", "priority": "Normal"},
                {"invoice_id": "I3", "vendor_name": "Gamma", "amount": 50.0, "due_date": "2026-08-20",
                 "status": "Scheduled for Payment", "priority": "Normal"},
                {"invoice_id": "I2", "vendor_name": "Beta", "amount": 200.0, "due_date": "2026-09-19",
                 "status": "Pending Cash", "priority": "High"},
                {"invoice_id": "I1", "vendor_name": "Alpha", "amount": 100.0, "due_date": "2026-10-24",
                 "status": "Scheduled for Payment", "priority": "Normal"}
            ],
            "cash_requirements": {"total_payables": 450.0, "available_cash": 250.0,
                                  "cash_shortfall": 200.0, "credit_utilization": 22.5},
            "recommendations": [
                "3 overdue invoices require immediate attention",
                "Consider using credit facility to cover cash shortfall"
            ]
        }"#;
        assert_parity(
            analyze_accounts_payable(&payable_request(), "2026-11-03"),
            expected,
        );
    }

    #[test]
    fn unparseable_due_dates_fall_outside_every_bucket() {
        let mut request = payable_request();
        request["invoices"][0]["due_date"] = json!("not-a-date");
        let out = analyze_accounts_payable(&request, "2026-11-03");
        // The 100 falls out of "0-30" and lands in the residual instead.
        assert_eq!(out["aging_analysis"][0]["amount"], json!(0.0));
        assert_eq!(out["aging_analysis"][1]["amount"], json!(200.0));
        assert_eq!(out["aging_analysis"][2]["amount"], json!(250.0));
        assert_eq!(out["aging_analysis"][3]["amount"], json!(0.0));
    }

    fn receivable_request() -> Value {
        json!({
            "company_id": "C1",
            "target_dso": 30,
            "collection_budget": 500,
            "invoices": [
                {"invoice_id": "R1", "customer_id": "U1", "customer_name": "Ann", "invoice_date": "2026-10-24",
                 "due_date": "2026-11-23", "amount": 300, "amount_paid": 100, "status": "Current", "credit_limit": 1000},
                {"invoice_id": "R2", "customer_id": "U2", "customer_name": "Bob", "invoice_date": "2026-09-04",
                 "due_date": "2026-09-24", "amount": 200, "amount_paid": 0, "status": "Overdue 30", "credit_limit": 500},
                {"invoice_id": "R3", "customer_id": "U3", "customer_name": "Cid", "invoice_date": "2026-08-05",
                 "due_date": "2026-08-25", "amount": 150, "amount_paid": 50, "status": "Overdue 60", "credit_limit": 500},
                {"invoice_id": "R4", "customer_id": "U4", "customer_name": "Dee", "invoice_date": "2026-06-16",
                 "due_date": "2026-07-06", "amount": 100, "amount_paid": 0, "status": "Overdue 90", "credit_limit": 500}
            ]
        })
    }

    #[test]
    fn accounts_receivable_matches_the_service() {
        let expected = r#"{
            "company_id": "C1",
            "ar_summary": {"total_ar": 600.0, "invoice_count": 4},
            "aging_schedule": [
                {"bucket": "Current", "amount": 200.0, "percentage": 33.33},
                {"bucket": "1-30 Days Overdue", "amount": 200.0, "percentage": 33.33},
                {"bucket": "31-60 Days Overdue", "amount": 100.0, "percentage": 16.67},
                {"bucket": "61-90 Days Overdue", "amount": 100.0, "percentage": 16.67}
            ],
            "dso_analysis": {"actual_dso": 45.0, "target_dso": 30.0, "dso_variance": 15.0,
                             "collection_efficiency": -50.0},
            "collection_plan": [
                {"customer_id": "U3", "customer_name": "Cid", "outstanding_amount": 100.0,
                 "days_overdue": 30, "priority": "High", "recommended_action": "Escalate to legal"},
                {"customer_id": "U4", "customer_name": "Dee", "outstanding_amount": 100.0,
                 "days_overdue": 30, "priority": "High", "recommended_action": "Escalate to legal"}
            ],
            "bad_debt_provision": {"current_arrears": 110.0, "provision_percentage": 1833.33},
            "recommendations": [
                "DSO significantly above target - improve collection efforts",
                "High 90+ day receivables - escalate collection",
                "High bad debt risk - increase provisions"
            ]
        }"#;
        assert_parity(analyze_accounts_receivable(&receivable_request()), expected);
    }

    #[test]
    fn empty_receivables_divide_by_nothing() {
        let out = analyze_accounts_receivable(
            &json!({"company_id": "C1", "target_dso": 0, "invoices": []}),
        );
        assert_eq!(out["ar_summary"]["total_ar"], json!(0.0));
        assert_eq!(out["aging_schedule"][0]["percentage"], json!(0.0));
        assert_eq!(out["dso_analysis"]["collection_efficiency"], json!(0.0));
        assert_eq!(
            out["bad_debt_provision"]["provision_percentage"],
            json!(0.0)
        );
        // 45 > 0 * 1.2, so the DSO warning still fires on an empty book.
        assert_eq!(
            out["recommendations"],
            json!(["DSO significantly above target - improve collection efforts"])
        );
    }

    fn ledger_request() -> Value {
        json!({
            "company_id": "C1",
            "reporting_date": "2026-10-31",
            "accounts": [
                {"account_id": "A1", "account_name": "Cash", "account_type": "Asset", "balance": 500,
                 "debit_credit": "Debit", "parent_account": ""},
                {"account_id": "A2", "account_name": "Stock", "account_type": "Asset", "balance": 200.5,
                 "debit_credit": "Debit", "parent_account": ""},
                {"account_id": "L1", "account_name": "Loan", "account_type": "Liability", "balance": 300,
                 "debit_credit": "Credit", "parent_account": ""},
                {"account_id": "E1", "account_name": "Capital", "account_type": "Equity", "balance": 400,
                 "debit_credit": "Credit", "parent_account": ""},
                {"account_id": "R1", "account_name": "Sales", "account_type": "Revenue", "balance": 250,
                 "debit_credit": "Credit", "parent_account": ""},
                {"account_id": "X1", "account_name": "Rent", "account_type": "Expense", "balance": 260,
                 "debit_credit": "Debit", "parent_account": ""}
            ]
        })
    }

    #[test]
    fn general_ledger_matches_the_service() {
        let expected = r#"{
            "company_id": "C1",
            "trial_balance": {"total_debits": 960.5, "total_credits": 950.0, "balanced": false, "variance": 10.5},
            "account_analysis": [
                {"account_id": "A1", "name": "Cash", "type": "Asset", "balance": 500.0},
                {"account_id": "A2", "name": "Stock", "type": "Asset", "balance": 200.5},
                {"account_id": "L1", "name": "Loan", "type": "Liability", "balance": 300.0},
                {"account_id": "E1", "name": "Capital", "type": "Equity", "balance": 400.0},
                {"account_id": "R1", "name": "Sales", "type": "Revenue", "balance": 250.0},
                {"account_id": "X1", "name": "Rent", "type": "Expense", "balance": 260.0}
            ],
            "balance_sheet_totals": {"total_assets": 700.5, "total_liabilities": 300.0, "total_equity": 400.0},
            "income_statement_totals": {"total_revenue": 250.0, "total_expenses": 260.0},
            "recommendations": [
                "Trial balance is not balanced - review entries",
                "Net loss detected - review expense controls"
            ]
        }"#;
        assert_parity(analyze_general_ledger(&ledger_request()), expected);
    }

    fn journal_request() -> Value {
        json!({
            "company_id": "C1",
            "period_start": "2026-10-01",
            "period_end": "2026-10-31",
            "entries": [
                {"entry_id": "J1", "description": "Standard sale", "amount": 100, "entry_date": "2026-10-01",
                 "status": "Posted", "debit_account": "Cash", "credit_account": "Sales",
                 "source_document": "DOC", "posted_by": "sam"},
                {"entry_id": "J2", "description": "Adjusting accrual", "amount": 50, "entry_date": "2026-10-05",
                 "status": "Pending", "debit_account": "Rent", "credit_account": "Accruals",
                 "source_document": "DOC", "posted_by": "sam"},
                {"entry_id": "J3", "description": "Closing entry", "amount": 30, "entry_date": "2026-10-31",
                 "status": "Rejected", "debit_account": "Revenue", "credit_account": "Capital",
                 "source_document": "DOC", "posted_by": "sam"},
                {"entry_id": "J4", "description": "Reversing entry", "amount": 20, "entry_date": "2026-10-31",
                 "status": "Posted", "debit_account": "Accruals", "credit_account": "Rent",
                 "source_document": "DOC", "posted_by": "sam"}
            ]
        })
    }

    #[test]
    fn journal_entries_match_the_service() {
        let expected = r#"{
            "company_id": "C1",
            "entries_summary": {"total_entries": 4, "posted": 2, "pending": 1, "rejected": 1, "posting_rate": 50.0},
            "entries_by_type": {"standard_entries": 1, "adjusting_entries": 1, "closing_entries": 1,
                                "reversing_entries": 1},
            "pending_entries": [
                {"entry_id": "J2", "description": "Adjusting accrual", "amount": 50.0, "date": "2026-10-05"}
            ],
            "validation_results": {"debits_equals_credits": true, "total_debits": 200.0,
                                   "total_credits": 200.0, "variance": 0.0},
            "recommendations": ["High rejection rate - review entry procedures"]
        }"#;
        assert_parity(analyze_journal_entries(&journal_request()), expected);
    }

    #[test]
    fn journal_validation_can_never_disagree() {
        // Both sides sum the amount column, so an unbalanced book still passes.
        let request = json!({"company_id": "C1", "entries": [
            {"description": "Standard one-sided", "amount": 90, "status": "Posted"}
        ]});
        let out = analyze_journal_entries(&request);
        assert_eq!(
            out["validation_results"]["debits_equals_credits"],
            json!(true)
        );
        assert_eq!(out["validation_results"]["variance"], json!(0.0));
    }

    fn analytics_request() -> Value {
        json!({
            "company_id": "C1",
            "period": "2026-10",
            "financials": {"revenue": 1000, "cogs": 600, "net_income": 120, "total_assets": 2000,
                           "total_equity": 800, "total_liabilities": 1200, "current_assets": 600,
                           "current_liabilities": 400, "inventory": 150, "accounts_receivable": 200,
                           "operating_cash_flow": 250},
            "benchmark": {"industry_avg_roe": 0.12, "industry_avg_current_ratio": 1.5,
                          "industry_avg_debt_to_equity": 1.0, "industry_avg_net_margin": 0.1}
        })
    }

    #[test]
    fn analytics_matches_the_service() {
        let out = analyze_financials(&analytics_request());
        assert_eq!(
            out["profitability"],
            json!({"gross_margin": 12.0, "net_margin": 12.0, "roe": 15.0, "roa": 6.0})
        );
        assert_eq!(
            out["liquidity"],
            json!({"current_ratio": 1.5, "quick_ratio": 1.12, "cash_ratio": 0.62})
        );
        assert_eq!(
            out["solvency"],
            json!({"debt_to_equity": 1.5, "debt_to_assets": 60.0, "interest_coverage": 2.0})
        );
        assert_eq!(
            out["efficiency"],
            json!({"asset_turnover": 0.5, "inventory_turnover": 4.0,
                   "receivables_turnover": 5.0, "dso": 73.0})
        );
        // Two of four benchmarks cleared, and the flags stay numeric.
        assert_eq!(out["overall_score"], json!(50.0));
        assert_eq!(
            out["benchmark_comparison"]["roe"]["above_industry"],
            json!(1.0)
        );
        assert_eq!(
            out["benchmark_comparison"]["current_ratio"]["above_industry"],
            json!(0.0)
        );
        assert_eq!(
            out["insights"],
            json!([
                "High leverage - debt-to-equity ratio above 1.0",
                "DSO of 73.0 days suggests collection process needs improvement"
            ])
        );
    }

    #[test]
    fn empty_financials_report_zeroes_without_division() {
        let out =
            analyze_financials(&json!({"company_id": "C1", "financials": {}, "benchmark": {}}));
        assert_eq!(out["profitability"]["roe"], json!(0.0));
        assert_eq!(out["liquidity"]["current_ratio"], json!(0.0));
        assert_eq!(out["solvency"]["interest_coverage"], json!(0.0));
        assert_eq!(out["efficiency"]["dso"], json!(0.0));
        assert_eq!(out["overall_score"], json!(0.0));
        assert_eq!(
            out["insights"],
            json!(["Current ratio below 1.0 - potential short-term liquidity risk"])
        );
    }

    #[test]
    fn civil_date_arithmetic_crosses_months_and_years() {
        assert_eq!(days_between("2026-10-24", "2026-11-03"), Some(10));
        assert_eq!(days_between("2026-06-06", "2026-11-03"), Some(150));
        assert_eq!(days_between("2025-11-03", "2026-11-03"), Some(365));
        assert_eq!(days_between("2026-13-01", "2026-11-03"), None);
        assert_eq!(days_between("", "2026-11-03"), None);
    }
}
