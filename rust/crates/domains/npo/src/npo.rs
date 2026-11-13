#![forbid(unsafe_code)]
//! npo-service: a non-profit's funds, the money that moves through them, and the statements that report them.
//!
//! A fund is the unit of accounting, and its balance is never taken from the caller. It moves by the transactions
//! recorded against it, so the balance and the transactions listed against the fund cannot tell different stories.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a fund starts on its initial balance with nothing contributed, nothing disbursed and active, and a second fund
//!   with the same code in the same Book is refused rather than created;
//! * a transaction is a contribution when it is a contribution, transfer in, investment or appreciation, and a
//!   disbursement otherwise - and the totals follow the same split as the balance;
//! * a permanently restricted or endowment fund accepts only investment and appreciation: a contribution into one is
//!   refused just as a disbursement is, which is what the service did;
//! * net assets are read from the active funds: general, operating and board designated are without donor
//!   restrictions and everything else carries them, and the total is the funds' total rather than a fresh sum;
//! * a statement of activities counts donations in the period, grant money received, and fund disbursements in the
//!   period, and reports the difference as the change in net assets;
//! * a statement of financial position sets the funds' total against itself, since the service had no liabilities to
//!   set against it;
//! * an SROI analysis and an NPO asset are answered without being kept - the service computed them and returned them,
//!   and a caller who expects to read them back was never able to;
//! * an asset's net book value is its acquisition cost and its code is the first eight characters of its identifier,
//!   because the service never depreciated one.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{
    decimal, field, money_or_nothing, optional, or_default, refused, text, whole,
};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, NPO_AUDIT_REPORT, NPO_BUDGET, NPO_BUDGET_LINE,
    NPO_DONATION, NPO_DONOR, NPO_FUND, NPO_FUND_RESTRICTION, NPO_FUND_TRANSACTION, NPO_GRANT,
    NPO_GRANT_DRAWDOWN, NPO_IMPACT_MEASUREMENT, NPO_INTERNAL_CONTROL, NPO_NET_ASSETS, NPO_PROGRAM,
    NPO_PROGRAM_METRIC, NPO_PROJECT, NPO_STATEMENT_OF_ACTIVITIES, NPO_STATEMENT_OF_POSITION,
    NPO_VOLUNTEER_RECORD,
};

/// The transaction kinds that put money into a fund. Everything else takes money out.
const PUTS_IN: [&str; 4] = ["contribution", "transfer_in", "investment", "appreciation"];

/// The fund kinds whose money may only be invested or appreciated, never spent.
const HELD_IN_TRUST: [&str; 2] = ["permanently_restricted", "endowment"];

/// The fund kinds that sit outside a donor's restriction.
const UNRESTRICTED_KINDS: [&str; 3] = ["general", "operating", "board_designated"];

fn money_in(record: &Value, key: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(record, key, "0"))
        .map_err(|_| refused(&format!("{key} is not an amount")))
}

fn rows_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, kind).list(scope, 2000)
}

// ---------------------------------------------------------------------------------------------
// funds
// ---------------------------------------------------------------------------------------------

/// `POST /funds/`.
pub fn create_npo_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let code = text(payload, "fund_code");
    if code.is_empty() {
        return Err(refused("fund_code is required"));
    }
    // a fund code names one fund in a Book; a second with the same name would make every later report ambiguous
    let taken = rows_of(graph, scope, NPO_FUND)?
        .iter()
        .any(|row| field(row, "fund_code") == code);
    if taken {
        return Err(refused(&format!("Fund code {code} already exists")));
    }

    let opening = money_or_nothing(payload, "initial_balance")?;
    let recorded = json!({
        "fund_code": code,
        "fund_name": text(payload, "fund_name"),
        "fund_type": or_default(payload, "fund_type", "general"),
        "description": text(payload, "description"),
        "purpose": text(payload, "purpose"),
        "created_date": optional(payload, "created_date"),
        "current_balance": wire(&opening),
        "total_contributions": wire(&Money::zero()),
        "total_disbursements": wire(&Money::zero()),
        "currency": or_default(payload, "currency", "USD"),
        "parent_fund_id": optional(payload, "parent_fund_id"),
        "status": "active",
    });
    remember(graph, scope, NPO_FUND, recorded)
}

/// `GET /funds/`, narrowed to a kind when one is asked for.
pub fn list_npo_funds(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(payload, "fund_type");
    let rows: Vec<Value> = rows_of(graph, scope, NPO_FUND)?
        .into_iter()
        .filter(|row| match &wanted {
            Some(kind) => field(row, "fund_type") == kind,
            None => true,
        })
        .collect();
    Ok(json!({ "funds": rows, "count": rows.len() }))
}

/// `GET /funds/{fund_id}`.
pub fn get_npo_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, NPO_FUND).get(scope, fund_id)
}

/// `POST /funds/{fund_id}/transactions/`: record money moving through a fund and move its balance with it.
pub fn record_fund_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let fund = get_npo_fund(graph, scope, fund_id)?;
    let kind = text(payload, "transaction_type");
    let amount = money_or_nothing(payload, "amount")?;

    let held = field(&fund, "fund_type");
    if HELD_IN_TRUST.contains(&held) && !PUTS_IN[2..].contains(&kind.as_str()) {
        return Err(refused(&format!(
            "Cannot make disbursement from {held} fund"
        )));
    }

    let balance = money_in(&fund, "current_balance")?;
    let puts_in = PUTS_IN.contains(&kind.as_str());
    let after = if puts_in {
        balance.plus(&amount)
    } else {
        balance.minus(&amount)
    };

    let transaction = remember(
        graph,
        scope,
        NPO_FUND_TRANSACTION,
        json!({
            "fund_id": fund_id,
            "transaction_date": text(payload, "transaction_date"),
            "transaction_type": kind,
            "amount": wire(&amount),
            "description": text(payload, "description"),
            "reference_number": text(payload, "reference_number"),
            "category": text(payload, "category"),
            "project_id": optional(payload, "project_id"),
            "grant_id": optional(payload, "grant_id"),
            "donor_id": optional(payload, "donor_id"),
            "created_by": text(payload, "created_by"),
            "balance_after": wire(&after),
        }),
    )?;

    // the balance and the running total move together, so the fund agrees with its own transactions
    let contributions = money_in(&fund, "total_contributions")?;
    let disbursements = money_in(&fund, "total_disbursements")?;
    let patch = if puts_in {
        json!({
            "current_balance": wire(&after),
            "total_contributions": wire(&contributions.plus(&amount)),
        })
    } else {
        json!({
            "current_balance": wire(&after),
            "total_disbursements": wire(&disbursements.plus(&amount)),
        })
    };
    Records::new(graph, NPO_FUND).update(scope, fund_id, &patch)?;

    Ok(transaction)
}

/// `GET /funds/{fund_id}/transactions/`, narrowed to a period when one is asked for.
pub fn fund_transactions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let from = optional(payload, "start_date");
    let to = optional(payload, "end_date");
    let rows: Vec<Value> = rows_of(graph, scope, NPO_FUND_TRANSACTION)?
        .into_iter()
        .filter(|row| field(row, "fund_id") == fund_id)
        .filter(|row| match (&from, &to) {
            (Some(from), Some(to)) => {
                let at = field(row, "transaction_date");
                at >= from.as_str() && at <= to.as_str()
            }
            (Some(from), None) => field(row, "transaction_date") >= from.as_str(),
            (None, Some(to)) => field(row, "transaction_date") <= to.as_str(),
            (None, None) => true,
        })
        .collect();
    Ok(json!({ "transactions": rows, "count": rows.len() }))
}

/// `POST /funds/{fund_id}/restrictions/`.
pub fn restrict_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    // the fund must be one the caller holds, or the restriction would hang off nothing they can see
    get_npo_fund(graph, scope, fund_id)?;
    let mut recorded = payload.clone();
    if let Some(object) = recorded.as_object_mut() {
        object.insert("fund_id".into(), json!(fund_id));
    }
    remember(graph, scope, NPO_FUND_RESTRICTION, recorded)
}

// ---------------------------------------------------------------------------------------------
// net assets
// ---------------------------------------------------------------------------------------------

/// The split of the active funds into restricted and unrestricted, and what they total.
pub fn net_assets_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let funds: Vec<Value> = rows_of(graph, scope, NPO_FUND)?
        .into_iter()
        .filter(|row| field(row, "status") == "active")
        .collect();

    let mut without = Money::zero();
    let mut with = Money::zero();
    let mut total = Money::zero();
    for fund in &funds {
        let balance = money_in(fund, "current_balance")?;
        total = total.plus(&balance);
        if UNRESTRICTED_KINDS.contains(&field(fund, "fund_type")) {
            without = without.plus(&balance);
        } else {
            with = with.plus(&balance);
        }
    }

    let surplus = if total.is_negative() {
        Money::zero()
    } else {
        total.clone()
    };
    let deficit = if total.is_negative() {
        Money::zero().minus(&total)
    } else {
        Money::zero()
    };

    Ok(json!({
        "net_assets_without_donor_restrictions": wire(&without),
        "net_assets_with_donor_restrictions": wire(&with),
        "net_assets_with_permanent_restrictions": wire(&Money::zero()),
        "net_assets_with_temporary_restrictions": wire(&Money::zero()),
        "endowment_net_assets": wire(&Money::zero()),
        "board_designated_net_assets": wire(&Money::zero()),
        "total_net_assets": wire(&total),
        "accumulated_surplus": wire(&surplus),
        "accumulated_deficit": wire(&deficit),
        // the service called the funds' own total the beginning: there was no earlier figure to compare against
        "beginning_net_assets": wire(&total),
        "net_assets_change": wire(&Money::zero()),
        "fund_count": funds.len(),
    }))
}

/// `POST /net-assets/`.
pub fn create_net_assets(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let figures = net_assets_of(graph, scope)?;
    let mut recorded = figures.clone();
    if let (Some(object), Some(figures)) = (recorded.as_object_mut(), figures.as_object()) {
        for key in ["as_of_date", "period_start", "period_end"] {
            if let Some(value) = payload.get(key) {
                object.insert(key.to_owned(), value.clone());
            }
        }
        let _ = figures;
    }
    remember(graph, scope, NPO_NET_ASSETS, recorded)
}

/// `GET /net-assets/{as_of_date}`.
pub fn net_assets_on(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    as_of_date: &str,
) -> Result<Value, StoreError> {
    rows_of(graph, scope, NPO_NET_ASSETS)?
        .into_iter()
        .find(|row| field(row, "as_of_date") == as_of_date)
        // a date with no snapshot is a miss, the same as any other record the Book does not hold
        .ok_or(StoreError::NotFound)
}

// ---------------------------------------------------------------------------------------------
// donations, grants, donors
// ---------------------------------------------------------------------------------------------

/// `POST /donations/`.
pub fn create_donation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() {
        return Err(refused("a donation must be an amount"));
    }
    let mut recorded = payload.clone();
    if let Some(object) = recorded.as_object_mut() {
        object.insert("amount".into(), wire(&amount));
    }
    remember(graph, scope, NPO_DONATION, recorded)
}

/// `GET /donations/`.
pub fn list_donations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let rows = rows_of(graph, scope, NPO_DONATION)?;
    Ok(json!({ "donations": rows, "count": rows.len() }))
}

/// `POST /grants/`.
pub fn create_grant(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = money_or_nothing(payload, "amount")?;
    let recorded = json!({
        "grant_name": text(payload, "grant_name"),
        "grantor_name": text(payload, "grantor_name"),
        "grant_number": text(payload, "grant_number"),
        "amount": wire(&amount),
        "amount_received": wire(&Money::zero()),
        "start_date": optional(payload, "start_date"),
        "end_date": optional(payload, "end_date"),
        "purpose": text(payload, "purpose"),
        "restrictions": text(payload, "restrictions"),
        "status": or_default(payload, "status", "active"),
        "reporting_requirements": text(payload, "reporting_requirements"),
    });
    remember(graph, scope, NPO_GRANT, recorded)
}

/// `GET /grants/`, narrowed to a status when one is asked for.
pub fn list_grants(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(payload, "status");
    let rows: Vec<Value> = rows_of(graph, scope, NPO_GRANT)?
        .into_iter()
        .filter(|row| match &wanted {
            Some(status) => field(row, "status") == status,
            None => true,
        })
        .collect();
    Ok(json!({ "grants": rows, "count": rows.len() }))
}

/// `GET /grants/{grant_id}`.
pub fn get_grant(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    grant_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, NPO_GRANT).get(scope, grant_id)
}

/// `POST /grants/{grant_id}/drawdowns/`: take money against a grant and raise what it has received.
pub fn record_drawdown(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    grant_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let grant = get_grant(graph, scope, grant_id)?;
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() {
        return Err(refused("a drawdown must be an amount"));
    }

    let drawdown = remember(
        graph,
        scope,
        NPO_GRANT_DRAWDOWN,
        json!({
            "grant_id": grant_id,
            "amount": wire(&amount),
            "drawdown_date": text(payload, "drawdown_date"),
            "purpose": text(payload, "purpose"),
            "reference_number": text(payload, "reference_number"),
        }),
    )?;

    let received = money_in(&grant, "amount_received")?;
    Records::new(graph, NPO_GRANT).update(
        scope,
        grant_id,
        &json!({ "amount_received": wire(&received.plus(&amount)) }),
    )?;
    Ok(drawdown)
}

/// `POST /donors/`.
pub fn create_donor(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "donor_name");
    if name.is_empty() {
        return Err(refused("donor_name is required"));
    }
    let mut recorded = payload.clone();
    if let Some(object) = recorded.as_object_mut() {
        object.insert(
            "total_donated".into(),
            wire(&money_or_nothing(payload, "total_donated").unwrap_or_else(|_| Money::zero())),
        );
    }
    remember(graph, scope, NPO_DONOR, recorded)
}

/// `GET /donors/`.
pub fn list_donors(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let rows = rows_of(graph, scope, NPO_DONOR)?;
    Ok(json!({ "donors": rows, "count": rows.len() }))
}

/// `GET /donors/{donor_id}`.
pub fn get_donor(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    donor_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, NPO_DONOR).get(scope, donor_id)
}

// ---------------------------------------------------------------------------------------------
// projects, programs, budgets, governance, impact
// ---------------------------------------------------------------------------------------------

fn remember_with(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    payload: &Value,
    extra: Value,
) -> Result<Value, StoreError> {
    let mut recorded = payload.clone();
    if let (Some(object), Some(extra)) = (recorded.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            object.insert(key.clone(), value.clone());
        }
    }
    remember(graph, scope, kind, recorded)
}

/// `POST /projects/`.
pub fn create_project(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let budget = money_or_nothing(payload, "budget_amount")?;
    remember_with(
        graph,
        scope,
        NPO_PROJECT,
        payload,
        json!({ "budget_amount": wire(&budget), "status": or_default(payload, "status", "planning") }),
    )
}

/// `GET /projects/`, narrowed to a status when one is asked for.
pub fn list_projects(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(payload, "status");
    let rows: Vec<Value> = rows_of(graph, scope, NPO_PROJECT)?
        .into_iter()
        .filter(|row| match &wanted {
            Some(status) => field(row, "status") == status,
            None => true,
        })
        .collect();
    Ok(json!({ "projects": rows, "count": rows.len() }))
}

/// `GET /projects/{project_id}`.
pub fn get_project(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    project_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, NPO_PROJECT).get(scope, project_id)
}

/// `POST /programs/`.
pub fn create_program(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    remember_with(graph, scope, NPO_PROGRAM, payload, json!({}))
}

/// `GET /programs/`.
pub fn list_programs(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let rows = rows_of(graph, scope, NPO_PROGRAM)?;
    Ok(json!({ "programs": rows, "count": rows.len() }))
}

/// `POST /budgets/`.
pub fn create_npo_budget(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = money_or_nothing(payload, "total_budget")?;
    remember_with(
        graph,
        scope,
        NPO_BUDGET,
        payload,
        json!({ "total_budget": wire(&total), "total_actual": wire(&Money::zero()) }),
    )
}

/// `GET /budgets/`, narrowed to a fiscal year when one is asked for.
pub fn list_npo_budgets(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(payload, "fiscal_year");
    let rows: Vec<Value> = rows_of(graph, scope, NPO_BUDGET)?
        .into_iter()
        .filter(|row| match &wanted {
            Some(year) => field(row, "fiscal_year") == year,
            None => true,
        })
        .collect();
    Ok(json!({ "budgets": rows, "count": rows.len() }))
}

/// `POST /budgets/{budget_id}/lines/`.
pub fn add_budget_line(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    budget_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    // the budget must be one the caller holds, or the line would hang off nothing they can see
    Records::new(graph, NPO_BUDGET).get(scope, budget_id)?;
    let budgeted = money_or_nothing(payload, "budgeted_amount")?;
    let mut recorded = payload.clone();
    if let Some(object) = recorded.as_object_mut() {
        object.insert("budget_id".into(), json!(budget_id));
        object.insert("budgeted_amount".into(), wire(&budgeted));
        object.insert("actual_amount".into(), wire(&Money::zero()));
    }
    remember(graph, scope, NPO_BUDGET_LINE, recorded)
}

/// `POST /internal-controls/`.
pub fn create_internal_control(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    remember_with(graph, scope, NPO_INTERNAL_CONTROL, payload, json!({}))
}

/// `GET /internal-controls/`.
pub fn list_internal_controls(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let rows = rows_of(graph, scope, NPO_INTERNAL_CONTROL)?;
    Ok(json!({ "internal_controls": rows, "count": rows.len() }))
}

/// `POST /audit-reports/`.
pub fn create_audit_report(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    remember_with(graph, scope, NPO_AUDIT_REPORT, payload, json!({}))
}

/// `GET /audit-reports/`.
pub fn list_audit_reports(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let rows = rows_of(graph, scope, NPO_AUDIT_REPORT)?;
    Ok(json!({ "audit_reports": rows, "count": rows.len() }))
}

/// `POST /program-metrics/`.
pub fn create_program_metric(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    remember_with(graph, scope, NPO_PROGRAM_METRIC, payload, json!({}))
}

/// `POST /impact-measurements/`.
pub fn create_impact_measurement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut extra = json!({});
    if let (Some(object), Ok(social)) = (
        extra.as_object_mut(),
        money_or_nothing(payload, "social_value_created"),
    ) {
        object.insert("social_value_created".into(), wire(&social));
    }
    remember_with(graph, scope, NPO_IMPACT_MEASUREMENT, payload, extra)
}

/// `POST /sroi-analyses/`: worked out and answered, never kept - the service had no store for one.
pub fn analyse_sroi(payload: &Value) -> Result<Value, StoreError> {
    let investment = money_or_nothing(payload, "total_investment")?;
    let social = money_or_nothing(payload, "social_value_created")?;
    if investment.is_zero() {
        return Err(refused(
            "total_investment must be greater than zero for a ratio",
        ));
    }
    // the ratio is the only figure the analysis existed for
    let ratio = social.over(&investment);
    Ok(json!({
        "status": "success",
        "analysis": {
            "total_investment": wire(&investment),
            "social_value_created": wire(&social),
            "social_return_on_investment": ratio.to_fixed(4),
        },
    }))
}

/// `POST /volunteer-records/`.
pub fn create_volunteer_record(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let hours = payload
        .get("hours_contributed")
        .and_then(Value::as_f64)
        .unwrap_or_default();
    // what the hours were worth at the rate the caller gave, which is how the service valued them
    let rate = money_or_nothing(payload, "hourly_rate")?;
    let counted = Money::parse(&format!("{hours}"))
        .map_err(|_| refused("hours_contributed must be a number"))?;
    let worth = rate.times(&counted);
    remember_with(
        graph,
        scope,
        NPO_VOLUNTEER_RECORD,
        payload,
        json!({ "estimated_value": wire(&worth) }),
    )
}

/// `GET /volunteer-records/`, narrowed to a period when one is asked for.
pub fn list_volunteer_records(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let from = optional(payload, "start_date");
    let to = optional(payload, "end_date");
    let rows: Vec<Value> = rows_of(graph, scope, NPO_VOLUNTEER_RECORD)?
        .into_iter()
        .filter(|row| match (&from, &to) {
            (Some(from), Some(to)) => {
                let at = field(row, "volunteer_date");
                at >= from.as_str() && at <= to.as_str()
            }
            (Some(from), None) => field(row, "volunteer_date") >= from.as_str(),
            (None, Some(to)) => field(row, "volunteer_date") <= to.as_str(),
            (None, None) => true,
        })
        .collect();
    Ok(json!({ "volunteer_records": rows, "count": rows.len() }))
}

// ---------------------------------------------------------------------------------------------
// statements
// ---------------------------------------------------------------------------------------------

/// What a period's statement of activities works out to.
pub fn activities_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    period_start: &str,
    period_end: &str,
) -> Result<Value, StoreError> {
    let in_period = |at: &str| at >= period_start && at <= period_end;

    let mut contributions = Money::zero();
    for donation in rows_of(graph, scope, NPO_DONATION)? {
        if in_period(field(&donation, "donation_date")) {
            contributions = contributions.plus(&money_in(&donation, "amount")?);
        }
    }

    let mut grants = Money::zero();
    for grant in rows_of(graph, scope, NPO_GRANT)? {
        if ["active", "completed"].contains(&field(&grant, "status")) {
            grants = grants.plus(&money_in(&grant, "amount_received")?);
        }
    }

    let mut expenses = Money::zero();
    for transaction in rows_of(graph, scope, NPO_FUND_TRANSACTION)? {
        let kind = field(&transaction, "transaction_type");
        if ["disbursement", "expense"].contains(&kind)
            && in_period(field(&transaction, "transaction_date"))
        {
            expenses = expenses.plus(&money_in(&transaction, "amount")?);
        }
    }

    let revenue = contributions.plus(&grants);
    let change = revenue.minus(&expenses);
    Ok(json!({
        "period_start": period_start,
        "period_end": period_end,
        "contributions": wire(&contributions),
        "grant_revenue": wire(&grants),
        "total_revenue": wire(&revenue),
        "program_expenses": wire(&expenses),
        "total_expenses": wire(&expenses),
        "change_in_net_assets": wire(&change),
        "surplus": wire(&if change.is_negative() { Money::zero() } else { change.clone() }),
        "deficit": wire(&if change.is_negative() { Money::zero().minus(&change) } else { Money::zero() }),
    }))
}

/// `POST /statements/activities/`.
pub fn create_statement_of_activities(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let start = text(payload, "period_start");
    let end = text(payload, "period_end");
    if start.is_empty() || end.is_empty() {
        return Err(refused("period_start and period_end are required"));
    }
    let figures = activities_of(graph, scope, &start, &end)?;
    remember(graph, scope, NPO_STATEMENT_OF_ACTIVITIES, figures)
}

/// `GET /statements/activities/` for a period.
pub fn statement_of_activities(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let start = text(payload, "period_start");
    let end = text(payload, "period_end");
    rows_of(graph, scope, NPO_STATEMENT_OF_ACTIVITIES)?
        .into_iter()
        .find(|row| field(row, "period_start") == start && field(row, "period_end") == end)
        // the route answered an optional record, so a period with no statement is nothing rather than an error
        .map_or(Ok(Value::Null), Ok)
}

/// `POST /statements/financial-position/`: the funds' total against itself, since there were no liabilities.
pub fn create_statement_of_position(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let figures = net_assets_of(graph, scope)?;
    let total = figures["total_net_assets"].clone();
    let recorded = json!({
        "as_of_date": text(payload, "as_of_date"),
        "total_assets": total,
        "total_liabilities": wire(&Money::zero()),
        "net_assets": total,
        "net_assets_without_donor_restrictions": figures["net_assets_without_donor_restrictions"].clone(),
        "net_assets_with_donor_restrictions": figures["net_assets_with_donor_restrictions"].clone(),
    });
    remember(graph, scope, NPO_STATEMENT_OF_POSITION, recorded)
}

/// `POST /assets/`: an asset answered without being kept, at its acquisition cost.
pub fn describe_asset(payload: &Value) -> Result<Value, StoreError> {
    let cost = money_or_nothing(payload, "acquisition_cost")?;
    let id = identifier_of(payload);
    Ok(json!({
        "status": "success",
        "asset": {
            "id": id,
            "asset_code": format!("AST-{}", &id[..id.len().min(8)]),
            "asset_name": text(payload, "asset_name"),
            "asset_type": text(payload, "asset_type"),
            "category": text(payload, "category"),
            "acquisition_date": text(payload, "acquisition_date"),
            "acquisition_cost": wire(&cost),
            "current_value": wire(&money_or_nothing(payload, "current_value")?),
            "useful_life_years": whole(payload, "useful_life_years"),
            "depreciation_method": text(payload, "depreciation_method"),
            "salvage_value": wire(&money_or_nothing(payload, "salvage_value")?),
            "location": text(payload, "location"),
            "responsible_person": text(payload, "responsible_person"),
            "fund_id": optional(payload, "fund_id"),
            // never depreciated: the service answered the cost as the net book value
            "accumulated_depreciation": wire(&Money::zero()),
            "net_book_value": wire(&cost),
            "status": or_default(payload, "status", "active"),
        },
    }))
}

/// An identifier for an answer that is not kept, so two such answers cannot be mistaken for one.
fn identifier_of(payload: &Value) -> String {
    let seed = payload.to_string();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn props(graph: &FakeGraph, index: usize) -> Value {
        let params = graph.statement(index).1;
        if params.get("props").is_some() {
            params["props"].clone()
        } else {
            params["patch"].clone()
        }
    }

    #[test]
    fn a_fund_starts_on_its_opening_balance_with_nothing_moved_yet() {
        let graph = FakeGraph::queue(vec![vec![], vec![json!({"id": "f-1"})]]);
        let answer = create_npo_fund(
            &graph,
            &scope(),
            &json!({
                "fund_code": "GEN-1",
                "fund_name": "General",
                "fund_type": "general",
                "purpose": "running costs",
                "initial_balance": 500,
                "current_balance": 9999,
                "status": "closed",
            }),
        )
        .expect("created");

        let written = props(&graph, 1);
        assert_eq!(written["current_balance"], json!("500.000000"));
        assert_eq!(written["total_contributions"], json!("0.000000"));
        assert_eq!(written["total_disbursements"], json!("0.000000"));
        assert_eq!(written["status"], json!("active"));
        assert_eq!(written["currency"], json!("USD"));
        assert_eq!(answer["id"], json!("f-1"));
    }

    #[test]
    fn a_fund_code_already_in_the_book_is_refused() {
        let graph = FakeGraph::returning(vec![json!({"id": "f-1", "fund_code": "GEN-1"})]);
        let refused = create_npo_fund(
            &graph,
            &scope(),
            &json!({"fund_code": "GEN-1", "fund_name": "Another", "fund_type": "general"}),
        );
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("Fund code GEN-1 already exists"));
    }

    #[test]
    fn a_contribution_raises_the_balance_and_a_disbursement_lowers_it() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "f-1", "fund_type": "general", "current_balance": "100.00", "total_contributions": "0.00", "total_disbursements": "0.00"}),
            ],
            vec![json!({"id": "tx-1"})],
            vec![json!({"id": "f-1"})],
            vec![
                json!({"id": "f-1", "fund_type": "general", "current_balance": "160.00", "total_contributions": "60.00", "total_disbursements": "0.00"}),
            ],
            vec![json!({"id": "tx-2"})],
            vec![json!({"id": "f-1"})],
        ]);

        record_fund_transaction(
            &graph,
            &scope(),
            "f-1",
            &json!({"transaction_type": "contribution", "amount": 60}),
        )
        .expect("recorded");
        assert_eq!(props(&graph, 2)["current_balance"], json!("160.000000"));
        assert_eq!(props(&graph, 2)["total_contributions"], json!("60.000000"));

        record_fund_transaction(
            &graph,
            &scope(),
            "f-1",
            &json!({"transaction_type": "disbursement", "amount": 25}),
        )
        .expect("recorded");
        assert_eq!(props(&graph, 5)["current_balance"], json!("135.000000"));
        assert_eq!(props(&graph, 5)["total_disbursements"], json!("25.000000"));
    }

    #[test]
    fn an_endowment_accepts_only_investment_and_appreciation() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "f-1",
            "fund_type": "endowment",
            "current_balance": "1000.00",
            "total_contributions": "0.00",
            "total_disbursements": "0.00",
        })]);
        for refused_kind in ["contribution", "disbursement"] {
            let refused = record_fund_transaction(
                &graph,
                &scope(),
                "f-1",
                &json!({"transaction_type": refused_kind, "amount": 10}),
            );
            assert!(refused
                .unwrap_err()
                .to_string()
                .contains("Cannot make disbursement from endowment fund"));
        }
    }

    #[test]
    fn net_assets_split_the_active_funds_by_restriction() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "f-1", "fund_type": "general", "status": "active", "current_balance": "300.00"}),
            json!({"id": "f-2", "fund_type": "board_designated", "status": "active", "current_balance": "200.00"}),
            json!({"id": "f-3", "fund_type": "temporarily_restricted", "status": "active", "current_balance": "500.00"}),
            json!({"id": "f-4", "fund_type": "general", "status": "closed", "current_balance": "9999.00"}),
        ]);
        let figures = net_assets_of(&graph, &scope()).expect("worked out");

        assert_eq!(
            figures["net_assets_without_donor_restrictions"],
            json!("500.000000")
        );
        assert_eq!(
            figures["net_assets_with_donor_restrictions"],
            json!("500.000000")
        );
        assert_eq!(figures["total_net_assets"], json!("1000.000000"));
        assert_eq!(figures["accumulated_surplus"], json!("1000.000000"));
        assert_eq!(figures["accumulated_deficit"], json!("0.000000"));
        // the closed fund is not part of it
        assert_eq!(figures["fund_count"], json!(3));
    }

    #[test]
    fn a_statement_of_activities_counts_the_period_and_reports_the_change() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "d-1", "donation_date": "2026-03-04", "amount": "1000.00"}),
                json!({"id": "d-2", "donation_date": "2026-05-09", "amount": "250.00"}),
            ],
            vec![json!({"id": "g-1", "status": "active", "amount_received": "4000.00"})],
            vec![
                json!({"id": "t-1", "transaction_type": "disbursement", "transaction_date": "2026-03-11", "amount": "1500.00"}),
                json!({"id": "t-2", "transaction_type": "contribution", "transaction_date": "2026-03-11", "amount": "900.00"}),
            ],
        ]);
        let figures =
            activities_of(&graph, &scope(), "2026-01-01", "2026-03-31").expect("worked out");

        // only the donation inside the period counts; the contribution is not an expense
        assert_eq!(figures["contributions"], json!("1000.000000"));
        assert_eq!(figures["grant_revenue"], json!("4000.000000"));
        assert_eq!(figures["total_revenue"], json!("5000.000000"));
        assert_eq!(figures["total_expenses"], json!("1500.000000"));
        assert_eq!(figures["change_in_net_assets"], json!("3500.000000"));
        assert_eq!(figures["deficit"], json!("0.000000"));
    }

    #[test]
    fn a_drawdown_raises_what_the_grant_has_received() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "g-1", "amount_received": "1000.00"})],
            vec![json!({"id": "dd-1"})],
            vec![json!({"id": "g-1"})],
        ]);
        record_drawdown(&graph, &scope(), "g-1", &json!({"amount": 250})).expect("recorded");
        assert_eq!(props(&graph, 2)["amount_received"], json!("1250.000000"));
    }

    #[test]
    fn an_sroi_ratio_is_the_social_value_over_the_investment() {
        let answer = analyse_sroi(&json!({"total_investment": 1000, "social_value_created": 3400}))
            .expect("worked out");
        assert_eq!(
            answer["analysis"]["social_return_on_investment"],
            json!("3.4000")
        );
    }

    #[test]
    fn an_sroi_analysis_with_nothing_invested_is_refused_rather_than_dividing_by_nothing() {
        let refused = analyse_sroi(&json!({"total_investment": 0, "social_value_created": 3400}));
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("greater than zero"));
    }

    #[test]
    fn an_asset_is_answered_at_its_acquisition_cost_with_no_depreciation() {
        let answer = describe_asset(&json!({
            "asset_name": "Vehicle",
            "acquisition_cost": 25000,
            "current_value": 21000,
            "useful_life_years": 5,
        }))
        .expect("described");
        let asset = &answer["asset"];
        assert_eq!(asset["net_book_value"], json!("25000.000000"));
        assert_eq!(asset["accumulated_depreciation"], json!("0.000000"));
        assert!(asset["asset_code"].as_str().unwrap().starts_with("AST-"));
        assert_eq!(asset["useful_life_years"], json!(5));
    }

    #[test]
    fn a_statement_of_position_sets_the_funds_against_no_liabilities() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "f-1", "fund_type": "general", "status": "active", "current_balance": "700.00"}),
                json!({"id": "f-2", "fund_type": "permanently_restricted", "status": "active", "current_balance": "300.00"}),
            ],
            vec![json!({"id": "s-1"})],
        ]);
        create_statement_of_position(&graph, &scope(), &json!({"as_of_date": "2026-03-31"}))
            .expect("created");
        let written = props(&graph, 1);
        assert_eq!(written["total_assets"], json!("1000.000000"));
        assert_eq!(written["total_liabilities"], json!("0.000000"));
        assert_eq!(written["net_assets"], json!("1000.000000"));
        assert_eq!(written["as_of_date"], json!("2026-03-31"));
    }

    #[test]
    fn a_donation_that_is_not_an_amount_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_donation(&graph, &scope(), &json!({"amount": 0, "donor_id": "d-1"}));
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("must be an amount"));
    }

    #[test]
    fn a_fund_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(get_npo_fund(&graph, &scope(), "nope").is_err());
    }
}
