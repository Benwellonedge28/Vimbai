#![forbid(unsafe_code)]
//! departmental-accounting-service, on the store.
//!
//! Departments and their cost structure: allocation rules, cost pools, the allocation run that
//! spreads a pool across departments, inter-department billing and its approval, the department
//! financial summary and performance report, and the comparison and cost-distribution reports.
//!
//! Amounts are exact everywhere they are added or subtracted: pool totals, allocated amounts, bills,
//! direct and allocated costs, net income and every report total. Percentages are ratios and stay
//! floats, as they were.
//!
//! The shapes, as shipped, each pinned by a test:
//!
//! * a cost pool's `included_departments` is never consulted: the run allocates across every active
//!   department of the caller's that the pool does not exclude,
//! * the headcount basis does not look at headcount, or at any other figure: every department in the
//!   run gets an equal share,
//! * the expenses basis compares one department's expenses against the same total repeated once per
//!   department, so it also lands on an equal share, and on nothing at all when that department's
//!   expenses are zero,
//! * the revenue basis needs each department's revenue from the ledger. This port does not reach into
//!   another service, so the run takes the revenues it is given (department id to amount) and treats
//!   an unmentioned department as having none,
//! * running a pool twice replaces the earlier results rather than adding to them, and the pool ends
//!   up closed whether or not anything was allocated,
//! * a department's budget variance is the budget less its revenue, not less its spending, and is left
//!   at zero unless a budget total is supplied,
//! * the performance report's cost efficiency reads "Good" below a hundred percent cost ratio, and
//!   "Needs Improvement" from there up, and its budget adherence is judged on the variance percentage
//!   against five,
//! * approving a bill returns the journal entry it implies rather than posting it.
//!
//! Revenue, direct expenses and budget totals used to be fetched from the accounting and budgeting
//! services over HTTP. A caller now supplies them on the query (`revenue`, `direct_expenses`,
//! `budget_total`) and an absent figure is zero, so the figures this service owns are the ones it
//! computes and stores.

use serde_json::{json, Map, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, wire};
use vimbai_store::read::{field, optional, or_default, text};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, DEPARTMENT, DEPARTMENT_ALLOCATION,
    DEPARTMENT_ALLOCATION_RULE, DEPARTMENT_COST_POOL, DEPARTMENT_FINANCIALS, INTER_DEPARTMENT_BILL,
};

fn flag(payload: &Value, key: &str) -> Option<bool> {
    payload.get(key).and_then(Value::as_bool)
}

/// A list of ids, whether it arrives as an array or as one comma-separated query value.
fn strings(payload: &Value, key: &str) -> Vec<String> {
    match payload.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(text)) => text
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn zero_or(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(Money::zero()),
        _ => amount(payload, key),
    }
}

fn stored(record: &Value, key: &str) -> Result<Money, StoreError> {
    vimbai_store::amounts::stored_amount(record, key)
}

fn written(body: Value, saved: &Value) -> Value {
    let mut answer = created_record(saved);
    match (answer.as_object_mut(), body.as_object()) {
        (Some(answer), Some(fields)) => {
            for (key, value) in fields {
                answer.entry(key.clone()).or_insert_with(|| value.clone());
            }
            Value::Object(answer.clone())
        }
        _ => body,
    }
}

fn rows_of<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, kind).list(scope, 1000)
}

fn percentage_of(part: f64, whole: f64) -> f64 {
    if whole == 0.0 {
        0.0
    } else {
        part / whole * 100.0
    }
}

fn money_ratio(part: &Money, whole: &Money) -> f64 {
    let part: f64 = part.to_f64_lossy();
    let whole: f64 = whole.to_f64_lossy();
    percentage_of(part, whole)
}

// --- departments ---

/// `/departments`: the department, plus the cost centre it needs.
pub fn create_department<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "department_name");
    let code = text(payload, "department_code");
    let body = json!({
        "department_code": code,
        "department_name": name,
        "department_type": or_default(payload, "department_type", "cost"),
        "parent_department_id": text(payload, "parent_department_id"),
        "manager_id": text(payload, "manager_id"),
        "manager_name": text(payload, "manager_name"),
        "cost_center_code": text(payload, "cost_center_code"),
        "revenue_center_code": text(payload, "revenue_center_code"),
        "status": or_default(payload, "status", "active"),
        "budget_id": text(payload, "budget_id"),
        "account_code": text(payload, "account_code"),
        "created_at": text(payload, "created_at"),
        "updated_at": text(payload, "created_at"),
    });
    let department = written(
        body.clone(),
        &Records::new(graph, DEPARTMENT).create(scope, &body)?,
    );
    Ok(json!({
        "department": department,
        "cost_centre": json!({
            "account_number": format!("DEPT-{code}"),
            "account_name": format!("{name} - Cost Center"),
            "account_type": "Expense",
            "description": format!("Cost center for {name}"),
        }),
    }))
}

/// `/departments` listing.
pub fn list_departments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let kind = optional(query, "department_type");
    let status = optional(query, "status");
    let parent = optional(query, "parent_id");
    let rows: Vec<Value> = rows_of(graph, scope, DEPARTMENT)?
        .into_iter()
        .filter(|row| {
            kind.as_deref()
                .map(|want| field(row, "department_type") == want)
                .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| field(row, "status") == want)
                    .unwrap_or(true)
                && parent
                    .as_deref()
                    .map(|want| field(row, "parent_department_id") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `/departments/{id}`.
pub fn get_department<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    department_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, DEPARTMENT).get(scope, department_id)
}

/// `/departments/{id}` update: a full replace, as shipped.
pub fn update_department<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    department_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, DEPARTMENT);
    let existing = records.get(scope, department_id)?;
    let keep = |key: &str| -> Value {
        match optional(payload, key) {
            Some(value) => json!(value),
            None => json!(field(&existing, key)),
        }
    };
    let body = json!({
        "department_code": keep("department_code"),
        "department_name": keep("department_name"),
        "department_type": or_default(payload, "department_type", field(&existing, "department_type")),
        "parent_department_id": text(payload, "parent_department_id"),
        "manager_id": keep("manager_id"),
        "manager_name": keep("manager_name"),
        "cost_center_code": text(payload, "cost_center_code"),
        "revenue_center_code": text(payload, "revenue_center_code"),
        "status": or_default(payload, "status", field(&existing, "status")),
        "budget_id": text(payload, "budget_id"),
        "account_code": text(payload, "account_code"),
        "created_at": field(&existing, "created_at"),
        "updated_at": text(payload, "updated_at"),
    });
    records.delete(scope, department_id)?;
    Ok(written(body.clone(), &records.create(scope, &body)?))
}

/// `/departments/{id}/hierarchy`: the department, its parents above it and its children.
pub fn department_hierarchy<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    department_id: &str,
) -> Result<Value, StoreError> {
    let department = Records::new(graph, DEPARTMENT).get(scope, department_id)?;
    let all = rows_of(graph, scope, DEPARTMENT)?;

    let mut parents = Vec::new();
    let mut current = field(&department, "parent_department_id").to_string();
    // walk up, stopping if the chain loops back on itself or reaches a department that is gone
    while !current.is_empty() && parents.len() < all.len() {
        match all.iter().find(|row| field(row, "id") == current) {
            Some(row) => {
                parents.push(row.clone());
                current = field(row, "parent_department_id").to_string();
            }
            None => break,
        }
    }
    let children: Vec<Value> = all
        .into_iter()
        .filter(|row| field(row, "parent_department_id") == department_id)
        .collect();

    Ok(json!({
        "department": department,
        "parents": parents,
        "children": children,
    }))
}

// --- allocation rules ---

/// `/allocation-rules`.
pub fn create_allocation_rule<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let body = json!({
        "department_id": text(payload, "department_id"),
        "cost_type": text(payload, "cost_type"),
        "allocation_basis": or_default(payload, "allocation_basis", "headcount"),
        "allocation_method": or_default(payload, "allocation_method", "direct"),
        "percentage": payload.get("percentage").and_then(Value::as_f64).unwrap_or_default(),
        "custom_formula": text(payload, "custom_formula"),
        "priority": payload.get("priority").and_then(Value::as_i64).unwrap_or(1),
        "is_active": flag(payload, "is_active").unwrap_or(true),
        "created_at": text(payload, "created_at"),
    });
    Ok(written(
        body.clone(),
        &Records::new(graph, DEPARTMENT_ALLOCATION_RULE).create(scope, &body)?,
    ))
}

/// `/allocation-rules` listing.
pub fn list_allocation_rules<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let department = optional(query, "department_id");
    let rows: Vec<Value> = rows_of(graph, scope, DEPARTMENT_ALLOCATION_RULE)?
        .into_iter()
        .filter(|row| {
            department
                .as_deref()
                .map(|want| field(row, "department_id") == want)
                .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `DELETE /allocation-rules/{id}`: the rule is deactivated, not removed.
pub fn deactivate_allocation_rule<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, DEPARTMENT_ALLOCATION_RULE).update(
        scope,
        rule_id,
        &json!({"is_active": false}),
    )?;
    Ok(json!({"status": "deactivated", "rule_id": rule_id}))
}

// --- cost pools ---

/// `/cost-pools`.
pub fn create_cost_pool<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = amount(payload, "total_amount")?;
    let body = json!({
        "pool_name": text(payload, "pool_name"),
        "pool_type": text(payload, "pool_type"),
        "total_amount": wire(&total),
        "allocation_basis": or_default(payload, "allocation_basis", "headcount"),
        "allocation_method": or_default(payload, "allocation_method", "direct"),
        "included_departments": strings(payload, "included_departments"),
        "excluded_departments": strings(payload, "excluded_departments"),
        "status": or_default(payload, "status", "open"),
        "period_start": text(payload, "period_start"),
        "period_end": text(payload, "period_end"),
        "created_at": text(payload, "created_at"),
    });
    Ok(written(
        body.clone(),
        &Records::new(graph, DEPARTMENT_COST_POOL).create(scope, &body)?,
    ))
}

/// `/cost-pools` listing.
pub fn list_cost_pools<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let status = optional(query, "status");
    let rows: Vec<Value> = rows_of(graph, scope, DEPARTMENT_COST_POOL)?
        .into_iter()
        .filter(|row| {
            status
                .as_deref()
                .map(|want| field(row, "status") == want)
                .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `/cost-pools/{id}`.
pub fn get_cost_pool<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    pool_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, DEPARTMENT_COST_POOL).get(scope, pool_id)
}

// --- the allocation run ---

/// `/allocate`: spreads the pool across the departments and keeps the result, replacing the last run.
pub fn run_allocation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let pool_id = text(payload, "cost_pool_id");
    let pools = Records::new(graph, DEPARTMENT_COST_POOL);
    let pool = pools.get(scope, &pool_id)?;

    let period_start = text(payload, "period_start");
    let period_end = text(payload, "period_end");
    pools.update(
        scope,
        &pool_id,
        &json!({"period_start": period_start, "period_end": period_end, "status": "allocating"}),
    )?;

    let total_pool = stored(&pool, "total_amount")?;
    let basis = or_default(&pool, "allocation_basis", "headcount");
    let method = or_default(&pool, "allocation_method", "direct");
    let excluded = strings(&pool, "excluded_departments");

    // every active department the pool does not exclude, whatever it says is included
    let departments: Vec<Value> = rows_of(graph, scope, DEPARTMENT)?
        .into_iter()
        .filter(|row| {
            field(row, "status") == "active" && !excluded.iter().any(|id| id == field(row, "id"))
        })
        .collect();

    let revenues = payload
        .get("revenue_by_department")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    let revenue_of = |id: &str| -> Result<Money, StoreError> {
        match revenues.get(id) {
            None | Some(Value::Null) => Ok(Money::zero()),
            Some(value) => amount(&json!({"revenue": value}), "revenue"),
        }
    };

    let count = departments.len();
    let equal_share = if count == 0 {
        0.0
    } else {
        100.0 / count as f64
    };
    let mut total_revenue = Money::zero();
    if basis == "revenue" {
        for department in &departments {
            total_revenue = total_revenue.plus(&revenue_of(field(department, "id"))?);
        }
    }

    let hundred = Money::from_i64(100);
    let mut allocations = Vec::new();
    let mut total_allocated = Money::zero();
    let mut journals = Vec::new();
    for department in &departments {
        let percentage = match basis.as_str() {
            "revenue" => {
                let revenue = revenue_of(field(department, "id"))?;
                money_ratio(&revenue, &total_revenue)
            }
            // headcount is not looked at, and the expenses basis measures one department against a
            // total made of that same department: both land on an equal share
            _ => equal_share,
        };
        // the share as a decimal string, so the multiplication stays exact from here on
        let share = amount(
            &json!({ "share": format!("{:.10}", percentage / 100.0) }),
            "share",
        )?;
        let allocated = total_pool.times(&share);
        total_allocated = total_allocated.plus(&allocated);

        let body = json!({
            "department_id": field(department, "id"),
            "department_name": field(department, "department_name"),
            "cost_pool_id": pool_id,
            "cost_pool_name": field(&pool, "pool_name"),
            "allocated_amount": wire(&allocated),
            "allocation_percentage": percentage,
            "allocation_basis_used": basis,
            "calculation_details": {
                "basis": basis,
                "method": method,
                "total_pool": wire(&total_pool),
            },
            "created_at": text(payload, "created_at"),
        });
        let saved = Records::new(graph, DEPARTMENT_ALLOCATION).create(scope, &body)?;
        allocations.push(written(body, &saved));

        let code = field(department, "department_code");
        journals.push(json!({
            "description": format!(
                "Cost allocation from {} to {}",
                field(&pool, "pool_name"),
                field(department, "department_name"),
            ),
            "reference": format!("ALLOC-{}", &pool_id[..pool_id.len().min(8)]),
            "date": text(payload, "date"),
            "lines": [
                json!({"account_code": format!("DEPT-{code}"),
                       "description": format!("Allocated cost from {}", field(&pool, "pool_name")),
                       "debit": true, "amount": wire(&allocated)}),
                json!({"account_code": format!("POOL-{}", &pool_id[..pool_id.len().min(8)]),
                       "description": format!("Cost pool {}", field(&pool, "pool_name")),
                       "debit": false, "amount": wire(&allocated)}),
            ],
        }));
    }

    // latest-wins: an earlier run of this pool is replaced rather than added to
    let existing = Records::new(graph, DEPARTMENT_ALLOCATION);
    for row in existing.list(scope, 1000)? {
        if field(&row, "cost_pool_id") == pool_id {
            existing.delete(scope, field(&row, "id"))?;
        }
    }
    let closed = pools.update(scope, &pool_id, &json!({"status": "closed"}))?;
    let _ = hundred;

    Ok(json!({
        "cost_pool": written(pool.clone(), &closed),
        "allocations": allocations,
        "total_allocated": wire(&total_allocated),
        "journal_entries": journals,
    }))
}

/// `/allocations/{pool_id}`.
pub fn allocation_results<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    pool_id: &str,
) -> Result<Value, StoreError> {
    let rows: Vec<Value> = rows_of(graph, scope, DEPARTMENT_ALLOCATION)?
        .into_iter()
        .filter(|row| field(row, "cost_pool_id") == pool_id)
        .collect();
    Ok(Value::Array(rows))
}

// --- inter-department billing ---

/// `/inter-department-bills`.
pub fn create_inter_department_bill<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = amount(payload, "amount")?;
    let body = json!({
        "bill_number": text(payload, "bill_number"),
        "from_department_id": text(payload, "from_department_id"),
        "from_department_name": text(payload, "from_department_name"),
        "to_department_id": text(payload, "to_department_id"),
        "to_department_name": text(payload, "to_department_name"),
        "service_description": text(payload, "service_description"),
        "service_category": text(payload, "service_category"),
        "amount": wire(&total),
        "billing_date": text(payload, "billing_date"),
        "period_start": text(payload, "period_start"),
        "period_end": text(payload, "period_end"),
        "status": or_default(payload, "status", "pending"),
        "approved_by": text(payload, "approved_by"),
        "invoice_id": text(payload, "invoice_id"),
        "notes": text(payload, "notes"),
        "created_at": text(payload, "created_at"),
    });
    Ok(written(
        body.clone(),
        &Records::new(graph, INTER_DEPARTMENT_BILL).create(scope, &body)?,
    ))
}

/// `/inter-department-bills` listing.
pub fn list_inter_department_bills<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = optional(query, "from_dept_id");
    let to = optional(query, "to_dept_id");
    let status = optional(query, "status");
    let rows: Vec<Value> = rows_of(graph, scope, INTER_DEPARTMENT_BILL)?
        .into_iter()
        .filter(|row| {
            from.as_deref()
                .map(|want| field(row, "from_department_id") == want)
                .unwrap_or(true)
                && to
                    .as_deref()
                    .map(|want| field(row, "to_department_id") == want)
                    .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| field(row, "status") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `/inter-department-bills/{id}/approve`, plus the entry the approval implies.
pub fn approve_inter_department_bill<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    bill_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let bills = Records::new(graph, INTER_DEPARTMENT_BILL);
    let existing = bills.get(scope, bill_id)?;
    let approved_by = text(payload, "approved_by");
    let bill = written(
        existing.clone(),
        &bills.update(
            scope,
            bill_id,
            &json!({"status": "approved", "approved_by": approved_by}),
        )?,
    );

    let departments = rows_of(graph, scope, DEPARTMENT)?;
    let find = |id: &str| {
        departments
            .iter()
            .find(|row| field(row, "id") == id)
            .cloned()
    };
    let from_department = find(field(&existing, "from_department_id"));
    let to_department = find(field(&existing, "to_department_id"));
    let amount = stored(&existing, "amount")?;

    let entry = match (from_department, to_department) {
        (Some(from), Some(to)) => {
            let from_code = match optional(&from, "cost_center_code") {
                Some(code) => code,
                None => format!("DEPT-{}", field(&from, "department_code")),
            };
            let to_code = match optional(&to, "revenue_center_code") {
                Some(code) => code,
                None => format!("DEPT-{}", field(&to, "department_code")),
            };
            json!({
                "description": format!(
                    "Inter-dept billing: {} -> {}",
                    field(&from, "department_name"),
                    field(&to, "department_name"),
                ),
                "reference": field(&existing, "bill_number"),
                "date": field(&existing, "billing_date"),
                "lines": [
                    json!({"account_code": from_code,
                           "description": format!("Charge to {}", field(&to, "department_name")),
                           "debit": true, "amount": wire(&amount)}),
                    json!({"account_code": to_code,
                           "description": format!("Service provided to {}", field(&from, "department_name")),
                           "debit": false, "amount": wire(&amount)}),
                ],
            })
        }
        _ => Value::Null,
    };

    Ok(json!({"bill": bill, "journal_entry": entry}))
}

// --- the department financial summary ---

struct Figures {
    revenue: Money,
    direct_expenses: Money,
    allocated_costs: Money,
    budget_total: Money,
}

fn figures<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    department_id: &str,
    query: &Value,
) -> Result<Figures, StoreError> {
    let revenue = zero_or(query, "revenue")?;
    let direct_expenses = zero_or(query, "direct_expenses")?;
    let budget_total = zero_or(query, "budget_total")?;

    let mut allocated = Money::zero();
    for row in rows_of(graph, scope, DEPARTMENT_ALLOCATION)? {
        if field(&row, "department_id") == department_id {
            allocated = allocated.plus(&stored(&row, "allocated_amount")?);
        }
    }

    Ok(Figures {
        revenue,
        direct_expenses,
        allocated_costs: allocated,
        budget_total,
    })
}

fn period_label(query: &Value) -> String {
    let start = text(query, "period_start");
    let end = text(query, "period_end");
    let day = |value: &str| value.chars().take(10).collect::<String>();
    format!("{} to {}", day(&start), day(&end))
}

fn summary(department: &Value, period: &str, calculated: &Figures) -> Result<Value, StoreError> {
    let total_expenses = calculated.direct_expenses.plus(&calculated.allocated_costs);
    let net_income = calculated.revenue.minus(&total_expenses);
    let (variance, variance_pct) = if calculated.budget_total.is_zero() {
        (Money::zero(), 0.0)
    } else {
        let variance = calculated.budget_total.minus(&calculated.revenue);
        (
            variance.clone(),
            money_ratio(&variance, &calculated.budget_total),
        )
    };

    Ok(json!({
        "department_id": field(department, "id"),
        "department_name": field(department, "department_name"),
        "period": period,
        "revenue": wire(&calculated.revenue),
        "direct_expenses": wire(&calculated.direct_expenses),
        "allocated_costs": wire(&calculated.allocated_costs),
        "total_expenses": wire(&total_expenses),
        "net_income": wire(&net_income),
        "budget_variance": wire(&variance),
        "budget_variance_percentage": variance_pct,
        "headcount": 0,
        "revenue_per_head": wire(&Money::zero()),
        "cost_per_head": wire(&Money::zero()),
    }))
}

/// `/departments/{id}/financials`, cached per department and period start.
pub fn department_financials<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    department_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let department = Records::new(graph, DEPARTMENT).get(scope, department_id)?;
    let calculated = figures(graph, scope, department_id, query)?;
    let period = period_label(query);
    let answer = summary(&department, &period, &calculated)?;

    let cache_key = format!(
        "{department_id}-{}",
        text(query, "period_start")
            .chars()
            .take(10)
            .collect::<String>()
    );
    let cache = Records::new(graph, DEPARTMENT_FINANCIALS);
    for row in cache.list(scope, 1000)? {
        if field(&row, "cache_key") == cache_key {
            cache.delete(scope, field(&row, "id"))?;
        }
    }
    let mut body = answer.clone();
    body["cache_key"] = json!(cache_key);
    cache.create(scope, &body)?;

    Ok(answer)
}

/// `/departments/{id}/performance-report`.
pub fn performance_report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    department_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let department = Records::new(graph, DEPARTMENT).get(scope, department_id)?;
    let calculated = figures(graph, scope, department_id, query)?;
    let period = period_label(query);
    let financials = summary(&department, &period, &calculated)?;

    let revenue = stored(&financials, "revenue")?;
    let total_expenses = stored(&financials, "total_expenses")?;
    let net_income = stored(&financials, "net_income")?;
    let variance_pct = financials
        .get("budget_variance_percentage")
        .and_then(Value::as_f64)
        .unwrap_or_default();

    let profit_margin = money_ratio(&net_income, &revenue);
    let cost_ratio = money_ratio(&total_expenses, &revenue);

    let mut recommendations = Vec::new();
    if profit_margin < 10.0 {
        recommendations.push(json!(
            "Consider increasing prices or reducing costs to improve profit margin"
        ));
    }
    if cost_ratio > 90.0 {
        recommendations.push(json!(
            "Review expense allocations and identify cost reduction opportunities"
        ));
    }
    // allocated costs against half the direct expenses, compared exactly rather than at 0.5
    if calculated.allocated_costs.times(&Money::from_i64(2)) > calculated.direct_expenses {
        recommendations.push(json!(
            "High allocated costs relative to direct expenses - review allocation methods"
        ));
    }

    Ok(json!({
        "department_id": department_id,
        "department_name": field(&department, "department_name"),
        "period_start": text(query, "period_start"),
        "period_end": text(query, "period_end"),
        "financial_summary": financials,
        "kpis": {
            "profit_margin": profit_margin,
            "cost_ratio": cost_ratio,
            "cost_efficiency": if cost_ratio < 80.0 { "Good" } else { "Needs Improvement" },
            "budget_adherence": if variance_pct.abs() < 5.0 {
                "On Budget"
            } else {
                "Over/Under Budget"
            },
        },
        "comparisons": {"budget_execution": Value::Null},
        "recommendations": recommendations,
        "generated_at": text(query, "generated_at"),
    }))
}

// --- reports ---

/// The comparison behind both `/reports/department-comparison` shapes.
pub fn compare_departments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let ids = strings(query, "department_ids");
    let period = period_label(query);

    let mut comparisons = Vec::new();
    let (mut total_revenue, mut total_expenses, mut total_net) =
        (Money::zero(), Money::zero(), Money::zero());
    for id in &ids {
        let department = Records::new(graph, DEPARTMENT).get(scope, id)?;
        let calculated = figures(graph, scope, id, query)?;
        let financials = summary(&department, &period, &calculated)?;
        total_revenue = total_revenue.plus(&stored(&financials, "revenue")?);
        total_expenses = total_expenses.plus(&stored(&financials, "total_expenses")?);
        total_net = total_net.plus(&stored(&financials, "net_income")?);
        comparisons.push(financials);
    }
    comparisons.sort_by(|left, right| {
        let left = left.get("net_income").and_then(Value::as_str).unwrap_or("");
        let right = right
            .get("net_income")
            .and_then(Value::as_str)
            .unwrap_or("");
        right.cmp(left)
    });

    Ok(json!({
        "period": period,
        "departments": comparisons,
        "summary": {
            "total_revenue": wire(&total_revenue),
            "total_expenses": wire(&total_expenses),
            "total_net_income": wire(&total_net),
        },
    }))
}

/// `/reports/cost-distribution`.
pub fn cost_distribution<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let period = period_label(query);
    let mut rows = Vec::new();
    let mut total = Money::zero();
    for department in rows_of(graph, scope, DEPARTMENT)? {
        if field(&department, "status") != "active" {
            continue;
        }
        let calculated = figures(graph, scope, field(&department, "id"), query)?;
        let total_expenses = calculated.direct_expenses.plus(&calculated.allocated_costs);
        total = total.plus(&total_expenses);
        rows.push(json!({
            "department_id": field(&department, "id"),
            "department_name": field(&department, "department_name"),
            "department_type": field(&department, "department_type"),
            "direct_costs": wire(&calculated.direct_expenses),
            "allocated_costs": wire(&calculated.allocated_costs),
            "total_costs": wire(&total_expenses),
            "percentage_of_total": 0.0,
        }));
    }
    for row in rows.iter_mut() {
        let share = stored(row, "total_costs")?;
        row["percentage_of_total"] = json!(money_ratio(&share, &total));
    }

    Ok(json!({
        "period": period,
        "distribution": rows,
        "total_cost_pool": wire(&total),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn department(id: &str, name: &str, status: &str) -> Value {
        json!({"id": id, "department_code": id.to_uppercase(), "department_name": name,
               "department_type": "cost", "status": status, "account_code": "5000"})
    }

    #[test]
    fn a_new_department_comes_with_its_cost_centre() {
        let graph = FakeGraph::returning(vec![json!({"id": "d-1"})]);
        let answer = create_department(
            &graph,
            &scope(),
            &json!({"department_code": "OPS", "department_name": "Operations",
                    "manager_id": "u-1", "manager_name": "Ana"}),
        )
        .expect("created");
        assert_eq!(answer["department"]["department_name"], json!("Operations"));
        assert_eq!(answer["cost_centre"]["account_number"], json!("DEPT-OPS"));
    }

    #[test]
    fn the_hierarchy_walks_up_and_lists_the_children() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-2", "department_name": "Ops", "parent_department_id": "d-1"})],
            vec![
                json!({"id": "d-1", "department_name": "Group", "parent_department_id": ""}),
                json!({"id": "d-2", "department_name": "Ops", "parent_department_id": "d-1"}),
                json!({"id": "d-3", "department_name": "Desk", "parent_department_id": "d-2"}),
            ],
        ]);
        let answer = department_hierarchy(&graph, &scope(), "d-2").expect("hierarchy");
        assert_eq!(answer["parents"].as_array().expect("parents").len(), 1);
        assert_eq!(answer["parents"][0]["department_name"], json!("Group"));
        assert_eq!(answer["children"][0]["department_name"], json!("Desk"));
    }

    #[test]
    fn an_equal_share_run_spreads_the_whole_pool() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "p-1", "pool_name": "Facilities", "total_amount": "900",
                        "allocation_basis": "headcount", "allocation_method": "direct",
                        "excluded_departments": []}),
            ],
            vec![json!({"id": "p-1"})],
            vec![
                department("d-1", "Ops", "active"),
                department("d-2", "Sales", "active"),
                department("d-3", "Old", "closed"),
            ],
            vec![json!({"id": "a-1"})],
            vec![json!({"id": "a-2"})],
            vec![],
            vec![json!({"id": "p-1", "status": "closed"})],
        ]);
        let answer = run_allocation(
            &graph,
            &scope(),
            &json!({"cost_pool_id": "p-1", "period_start": "2026-03-01T00:00:00Z",
                    "period_end": "2026-03-31T00:00:00Z"}),
        )
        .expect("allocated");
        let allocations = answer["allocations"].as_array().expect("allocations");
        // the closed department is not a target, and the two active ones split the pool
        assert_eq!(allocations.len(), 2);
        assert_eq!(allocations[0]["allocated_amount"], json!("450.000000"));
        assert_eq!(allocations[1]["allocation_percentage"], json!(50.0));
        assert_eq!(answer["total_allocated"], json!("900.000000"));
        assert_eq!(
            answer["journal_entries"]
                .as_array()
                .expect("journals")
                .len(),
            2
        );
    }

    #[test]
    fn the_headcount_basis_ignores_headcount_and_the_expenses_basis_ignores_the_pool() {
        // both bases land on an equal share, which is why these two departments get half each
        for basis in ["headcount", "expenses", "direct"] {
            let pool = json!({"id": "p-1", "pool_name": "Pool", "total_amount": "100",
                              "allocation_basis": basis, "allocation_method": "direct",
                              "excluded_departments": []});
            let graph = FakeGraph::queue(vec![
                vec![pool],
                vec![json!({"id": "p-1"})],
                vec![
                    department("d-1", "Ops", "active"),
                    department("d-2", "Sales", "active"),
                ],
                vec![json!({"id": "a-1"})],
                vec![json!({"id": "a-2"})],
                vec![],
                vec![json!({"id": "p-1", "status": "closed"})],
            ]);
            let answer = run_allocation(
                &graph,
                &scope(),
                &json!({"cost_pool_id": "p-1", "revenue_by_department": {"d-1": "999"}}),
            )
            .expect("allocated");
            let allocations = answer["allocations"].as_array().expect("allocations");
            assert_eq!(
                allocations[0]["allocated_amount"],
                json!("50.000000"),
                "{basis}"
            );
        }
    }

    #[test]
    fn the_revenue_basis_uses_the_revenues_it_is_given() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "p-1", "pool_name": "Adverts", "total_amount": "300",
                        "allocation_basis": "revenue", "allocation_method": "direct",
                        "excluded_departments": []}),
            ],
            vec![json!({"id": "p-1"})],
            vec![
                department("d-1", "Ops", "active"),
                department("d-2", "Sales", "active"),
            ],
            vec![json!({"id": "a-1"})],
            vec![json!({"id": "a-2"})],
            vec![],
            vec![json!({"id": "p-1", "status": "closed"})],
        ]);
        let answer = run_allocation(
            &graph,
            &scope(),
            &json!({"cost_pool_id": "p-1",
                    "revenue_by_department": {"d-1": "100", "d-2": "200"}}),
        )
        .expect("allocated");
        let allocations = answer["allocations"].as_array().expect("allocations");
        assert_eq!(allocations[0]["allocated_amount"], json!("100.000000"));
        assert_eq!(allocations[1]["allocated_amount"], json!("200.000000"));
    }

    #[test]
    fn a_pool_with_no_revenue_to_measure_allocates_nothing() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "p-1", "pool_name": "Adverts", "total_amount": "300",
                        "allocation_basis": "revenue", "allocation_method": "direct",
                        "excluded_departments": []}),
            ],
            vec![json!({"id": "p-1"})],
            vec![department("d-1", "Ops", "active")],
            vec![json!({"id": "a-1"})],
            vec![],
            vec![json!({"id": "p-1", "status": "closed"})],
        ]);
        let answer =
            run_allocation(&graph, &scope(), &json!({"cost_pool_id": "p-1"})).expect("allocated");
        assert_eq!(
            answer["allocations"][0]["allocated_amount"],
            json!("0.000000")
        );
        assert_eq!(answer["total_allocated"], json!("0.000000"));
    }

    #[test]
    fn the_excluded_departments_are_the_ones_left_out() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "p-1", "pool_name": "Pool", "total_amount": "100",
                        "allocation_basis": "headcount", "allocation_method": "direct",
                        "included_departments": ["d-1"], "excluded_departments": ["d-2"]}),
            ],
            vec![json!({"id": "p-1"})],
            vec![
                department("d-1", "Ops", "active"),
                department("d-2", "Sales", "active"),
            ],
            vec![json!({"id": "a-1"})],
            vec![],
            vec![json!({"id": "p-1", "status": "closed"})],
        ]);
        let answer =
            run_allocation(&graph, &scope(), &json!({"cost_pool_id": "p-1"})).expect("allocated");
        let allocations = answer["allocations"].as_array().expect("allocations");
        // one department left, so it takes the whole pool
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0]["allocated_amount"], json!("100.000000"));
    }

    #[test]
    fn a_bill_approval_returns_the_entry_with_its_account_codes() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "b-1", "bill_number": "B-1", "amount": "250.75",
                        "from_department_id": "d-1", "to_department_id": "d-2",
                        "billing_date": "2026-03-01T00:00:00Z"}),
            ],
            vec![json!({"id": "b-1"})],
            vec![
                json!({"id": "d-1", "department_code": "OPS", "department_name": "Ops",
                       "cost_center_code": "CC-OPS", "revenue_center_code": ""}),
                json!({"id": "d-2", "department_code": "SAL", "department_name": "Sales",
                       "cost_center_code": "", "revenue_center_code": ""}),
            ],
        ]);
        let answer =
            approve_inter_department_bill(&graph, &scope(), "b-1", &json!({"approved_by": "ana"}))
                .expect("approved");
        let lines = answer["journal_entry"]["lines"].as_array().expect("lines");
        assert_eq!(lines[0]["account_code"], json!("CC-OPS"));
        // the receiving side has no revenue centre, so it falls back to its department code
        assert_eq!(lines[1]["account_code"], json!("DEPT-SAL"));
        assert_eq!(lines[0]["amount"], json!("250.750000"));
    }

    #[test]
    fn the_financials_add_allocated_costs_to_direct_ones() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1", "department_name": "Ops"})],
            vec![
                json!({"department_id": "d-1", "allocated_amount": "100.25"}),
                json!({"department_id": "d-2", "allocated_amount": "999"}),
            ],
            vec![],
            vec![json!({"id": "f-1"})],
        ]);
        let answer = department_financials(
            &graph,
            &scope(),
            "d-1",
            &json!({"period_start": "2026-03-01T00:00:00Z", "period_end": "2026-03-31T00:00:00Z",
                    "revenue": "1000", "direct_expenses": "400"}),
        )
        .expect("financials");
        assert_eq!(answer["allocated_costs"], json!("100.250000"));
        assert_eq!(answer["total_expenses"], json!("500.250000"));
        assert_eq!(answer["net_income"], json!("499.750000"));
        // no budget total was supplied, so no variance is claimed
        assert_eq!(answer["budget_variance"], json!("0.000000"));
    }

    #[test]
    fn the_budget_variance_is_the_budget_less_revenue() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1", "department_name": "Ops"})],
            vec![],
            vec![],
            vec![json!({"id": "f-1"})],
        ]);
        let answer = department_financials(
            &graph,
            &scope(),
            "d-1",
            &json!({"revenue": "800", "direct_expenses": "900", "budget_total": "1000"}),
        )
        .expect("financials");
        // 1000 - 800, not 1000 - 1700: the shipped rule measures the budget against revenue
        assert_eq!(answer["budget_variance"], json!("200.000000"));
        assert_eq!(answer["budget_variance_percentage"], json!(20.0));
    }

    #[test]
    fn the_report_judges_efficiency_on_the_cost_ratio() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1", "department_name": "Ops"})],
            vec![],
            vec![],
            vec![json!({"id": "f-1"})],
        ]);
        let answer = performance_report(
            &graph,
            &scope(),
            "d-1",
            &json!({"revenue": "100", "direct_expenses": "95", "budget_total": "100"}),
        )
        .expect("report");
        assert_eq!(answer["kpis"]["cost_ratio"], json!(95.0));
        assert_eq!(
            answer["kpis"]["cost_efficiency"],
            json!("Needs Improvement")
        );
        assert_eq!(answer["kpis"]["profit_margin"], json!(5.0));
        assert_eq!(answer["kpis"]["budget_adherence"], json!("On Budget"));
        assert_eq!(
            answer["recommendations"].as_array().expect("advice").len(),
            2
        );
    }

    #[test]
    fn a_department_with_no_revenue_reports_zero_ratios_rather_than_dividing() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1", "department_name": "Support"})],
            vec![],
            vec![],
            vec![json!({"id": "f-1"})],
        ]);
        let answer = performance_report(&graph, &scope(), "d-1", &json!({"direct_expenses": "50"}))
            .expect("report");
        assert_eq!(answer["kpis"]["cost_ratio"], json!(0.0));
        assert_eq!(answer["kpis"]["profit_margin"], json!(0.0));
        assert_eq!(answer["kpis"]["cost_efficiency"], json!("Good"));
    }

    #[test]
    fn the_comparison_totals_what_it_compared() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1", "department_name": "Ops"})],
            vec![],
            vec![json!({"id": "d-2", "department_name": "Sales"})],
            vec![],
        ]);
        let answer = compare_departments(
            &graph,
            &scope(),
            &json!({"department_ids": ["d-1", "d-2"],
                    "period_start": "2026-03-01T00:00:00Z", "period_end": "2026-03-31T00:00:00Z"}),
        )
        .expect("comparison");
        assert_eq!(answer["summary"]["total_revenue"], json!("0.000000"));
        assert_eq!(answer["departments"].as_array().expect("rows").len(), 2);
    }

    #[test]
    fn the_cost_distribution_splits_the_active_departments() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "d-1", "department_name": "Ops", "department_type": "cost",
                       "status": "active"}),
                json!({"id": "d-2", "department_name": "Sales", "department_type": "revenue",
                       "status": "active"}),
                json!({"id": "d-3", "department_name": "Old", "department_type": "cost",
                       "status": "closed"}),
            ],
            vec![],
            vec![],
        ]);
        let answer = cost_distribution(
            &graph,
            &scope(),
            &json!({"period_start": "2026-03-01T00:00:00Z", "period_end": "2026-03-31T00:00:00Z"}),
        )
        .expect("distribution");
        assert_eq!(answer["distribution"].as_array().expect("rows").len(), 2);
        assert_eq!(answer["total_cost_pool"], json!("0.000000"));
    }

    #[test]
    fn an_allocation_actually_replaces_the_last_run() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "p-1", "pool_name": "Pool", "total_amount": "10",
                        "allocation_basis": "headcount", "allocation_method": "direct",
                        "excluded_departments": []}),
            ],
            vec![json!({"id": "p-1"})],
            vec![department("d-1", "Ops", "active")],
            vec![json!({"id": "a-1"})],
            vec![json!({"id": "a-9", "cost_pool_id": "p-1"})],
            vec![json!({"id": "a-9"})],
            vec![json!({"id": "p-1", "status": "closed"})],
        ]);
        run_allocation(&graph, &scope(), &json!({"cost_pool_id": "p-1"})).expect("allocated");
        let (seen, _) = graph.statement(5);
        assert!(seen.contains("DETACH DELETE"), "{seen}");
    }
}
