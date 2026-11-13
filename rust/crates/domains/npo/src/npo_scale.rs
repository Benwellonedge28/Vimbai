#![forbid(unsafe_code)]
//! npo-scale-service: the same non-profit accounting, run at multi-entity scale.
//!
//! An organisation here is one of five types - a non-profit, a sole-trader style commercial outfit, a partnership, a
//! private company or a PLC - and the type decides what the organisation gets: share capital for a company, partners'
//! capital accounts for a partnership, the fund set for a non-profit. The size band follows the money rather than a
//! choice: revenue decides the band, and a branch count or headcount can push an organisation up one, so growth is
//! never blocked by a classification.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an organisation is the caller's alone: a caller acting on an organisation they do not own gets the same miss as a
//!   nonexistent one, so the registry cannot be used to discover other tenants' organisations;
//! * a purchase is recorded on credit and settles only once: paying one that is already paid is refused, and paying it
//!   books the expense it was always going to become, so the reports never trail the bank;
//! * the creditors report is what is still owed right now, per vendor, largest first - unpaid purchases only;
//! * a size band is worked out from the organisation as it stands, never taken from a caller, and it is worked out
//!   again whenever the thing that decides it changes.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{
    decimal, field, money_or_nothing, optional, or_default, refused, text, whole,
};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, RecordKind, Records, StoreError, NPO_SCALE_BALANCE_ITEM, NPO_SCALE_BRANCH,
    NPO_SCALE_BUDGET, NPO_SCALE_COMPLIANCE, NPO_SCALE_DIRECTOR, NPO_SCALE_DIVIDEND,
    NPO_SCALE_DONATION, NPO_SCALE_DONOR, NPO_SCALE_EXPENSE, NPO_SCALE_ORG, NPO_SCALE_PARTNER,
    NPO_SCALE_PARTNER_DRAW, NPO_SCALE_PLEDGE, NPO_SCALE_PURCHASE, NPO_SCALE_RECEIPT,
    NPO_SCALE_REVENUE, NPO_SCALE_SHAREHOLDER, NPO_SCALE_VENDOR,
};

/// The organisation types the service accepts.
const ORG_TYPES: [&str; 5] = ["nonprofit", "commercial", "partnership", "company", "plc"];

/// The revenue ladder used for a non-profit.
/// Non-profit revenue bands in USD, from small up.
const NONPROFIT_BANDS: [(&str, u64); 4] = [
    ("small", 50000u64),
    ("medium", 500000u64),
    ("large", 5000000u64),
    ("extra_large", u64::MAX),
];

/// Partnership, company and PLC revenue bands in USD: they start at small.
const PARTNERSHIP_BANDS: [(&str, u64); 4] = [
    ("small", 500000u64),
    ("medium", 5000000u64),
    ("large", 50000000u64),
    ("extra_large", u64::MAX),
];

/// Commercial revenue bands in USD, from sole trader up.
const COMMERCIAL_BANDS: [(&str, u64); 5] = [
    ("sole_trader", 50000u64),
    ("small", 500000u64),
    ("medium", 5000000u64),
    ("large", 50000000u64),
    ("extra_large", u64::MAX),
];

/// Every distinct feature set, with the organisation type and size band that get it.
/// One feature set: the names a caller sees, and the organisation type and size band that get them.
type FeatureSet = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
);

const FEATURE_SETS: [FeatureSet; 21] = [
    (
        &[
            "core_accounting",
            "cash_book",
            "self_service_setup",
            "sales_receipting",
            "tax_calendar",
        ],
        &[("nonprofit", "sole_trader"), ("commercial", "sole_trader")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
        ],
        &[("nonprofit", "small")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
            "program_budgets",
            "recurring_donations",
            "impact_reporting",
        ],
        &[("nonprofit", "medium")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
            "program_budgets",
            "recurring_donations",
            "impact_reporting",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
        ],
        &[("nonprofit", "large")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
            "program_budgets",
            "recurring_donations",
            "impact_reporting",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "federation_chapters",
            "cross_border_consolidation",
        ],
        &[("nonprofit", "extra_large")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "cash_book",
            "sales_receipting",
            "tax_calendar",
        ],
        &[("company", "small")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
        ],
        &[("company", "medium")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "audit_trail",
        ],
        &[("company", "large")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "audit_trail",
            "subsidiaries",
            "group_consolidation",
            "intercompany",
        ],
        &[("company", "extra_large")],
    ),
    (
        &[
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "cash_book",
            "sales_receipting",
            "tax_calendar",
        ],
        &[("partnership", "small")],
    ),
    (
        &[
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "cash_book",
            "sales_receipting",
            "tax_calendar",
            "inventory_lite",
            "payroll",
            "multi_currency",
        ],
        &[("partnership", "medium")],
    ),
    (
        &[
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "multi_entity",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
        ],
        &[("partnership", "large")],
    ),
    (
        &[
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "multi_entity",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "intercompany",
            "group_consolidation",
            "joint_venture_accounts",
        ],
        &[("partnership", "extra_large")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
            "sales_receipting",
            "inventory_lite",
        ],
        &[("commercial", "small")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
            "program_budgets",
            "recurring_donations",
            "impact_reporting",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
        ],
        &[("commercial", "medium")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
            "program_budgets",
            "recurring_donations",
            "impact_reporting",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "multi_entity",
            "ifrs_reports",
        ],
        &[("commercial", "large")],
    ),
    (
        &[
            "core_accounting",
            "donor_crm",
            "automatic_receipting",
            "compliance_calendar",
            "program_budgets",
            "recurring_donations",
            "impact_reporting",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "federation_chapters",
            "cross_border_consolidation",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "multi_entity",
            "ifrs_reports",
            "intercompany",
            "group_consolidation",
        ],
        &[("commercial", "extra_large")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "cash_book",
            "sales_receipting",
            "tax_calendar",
            "public_share_registry",
        ],
        &[("plc", "small")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "public_share_registry",
            "mandatory_audit",
        ],
        &[("plc", "medium")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "audit_trail",
            "public_share_registry",
            "mandatory_audit",
            "public_disclosure",
        ],
        &[("plc", "large")],
    ),
    (
        &[
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "audit_trail",
            "subsidiaries",
            "group_consolidation",
            "intercompany",
            "public_share_registry",
            "mandatory_audit",
            "public_disclosure",
            "listing_compliance",
            "sec_filings",
        ],
        &[("plc", "extra_large")],
    ),
];

/// The size band at which a second approver is needed, and the limit below which one is not.
fn approval_limit(band: &str) -> Option<u64> {
    match band {
        "medium" => Some(5000),
        "large" => Some(2000),
        "extra_large" => Some(1000),
        // sole traders and small organisations approve alone
        _ => None,
    }
}

/// The size band an organisation falls into: revenue first, then branches and headcount pushing it up a band.
fn band_for(annual_revenue: u64, headcount: u64, branches: u64, org_type: &str) -> String {
    let ladder: &[(&str, u64)] = if ["partnership", "company", "plc"].contains(&org_type) {
        &PARTNERSHIP_BANDS
    } else if org_type == "commercial" {
        &COMMERCIAL_BANDS
    } else {
        &NONPROFIT_BANDS
    };
    let mut band = ladder[0].0;
    for (name, threshold) in ladder {
        if annual_revenue < *threshold {
            band = name;
            break;
        }
    }

    // an organisation that has grown its branches or its people is large whatever the revenue says
    if branches > 25 || headcount > 500 {
        "extra_large"
    } else if (branches > 5 || headcount > 50) && !["large", "extra_large"].contains(&band) {
        "large"
    } else {
        band
    }
    .to_owned()
}

/// The features an organisation type and band get.
fn features_for(org_type: &str, band: &str) -> Vec<&'static str> {
    for (names, who) in FEATURE_SETS {
        if who.contains(&(org_type, band)) {
            return names.to_vec();
        }
    }
    Vec::new()
}

fn rows_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: RecordKind,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, kind).list(scope, 2000)
}

fn revenue_of(org: &Value) -> u64 {
    decimal(org, "annual_revenue", "0")
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0)
}

/// Recompute an organisation's band from what it now is. The caller has already established that they hold it.
fn refresh_band(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org: &Value,
) -> Result<String, StoreError> {
    let org_id = field(org, "id");
    let branches = rows_of(graph, scope, NPO_SCALE_BRANCH)?
        .iter()
        .filter(|row| field(row, "org_id") == org_id)
        .count() as u64;
    let band = band_for(
        revenue_of(org),
        whole(org, "headcount") as u64,
        branches,
        field(org, "org_type"),
    );
    Records::new(graph, NPO_SCALE_ORG).update(scope, org_id, &json!({ "size_band": band }))?;
    Ok(band)
}

/// Fetch an organisation the caller owns, or answer as if it were not there.
fn held_org(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    let org = Records::new(graph, NPO_SCALE_ORG).get(scope, org_id)?;
    // ownership, not existence: another tenant's organisation answers the same as a nonexistent one
    if field(&org, "owner_id") != scope.user_id {
        return Err(StoreError::NotFound);
    }
    Ok(org)
}

/// `POST /orgs`.
pub fn create_org(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let org_type = or_default(payload, "org_type", "nonprofit");
    if !ORG_TYPES.contains(&org_type.as_str()) {
        return Err(refused(
            "org_type must be nonprofit, commercial, partnership, company or plc",
        ));
    }
    let revenue = money_or_nothing(payload, "annual_revenue")?;
    let headcount = whole(payload, "headcount");
    let band = band_for(
        revenue.to_f64_lossy() as u64,
        headcount as u64,
        0,
        &org_type,
    );

    let mut recorded = json!({
        "owner_id": scope.user_id,
        "name": text(payload, "name"),
        "org_type": org_type,
        "sector": or_default(payload, "sector", "community"),
        "country": or_default(payload, "country", "ZW"),
        "currency": or_default(payload, "currency", "USD"),
        "size_band": band,
        "annual_revenue": wire(&revenue),
        "headcount": headcount,
    });
    // the organisation's own Book can be linked later, so one is only carried when it is named
    if let (Some(object), Some(book)) = (recorded.as_object_mut(), optional(payload, "book_id")) {
        object.insert("book_id".into(), json!(book));
    }
    remember(graph, scope, NPO_SCALE_ORG, recorded)
}

/// `GET /orgs`, the caller's own only.
pub fn list_orgs(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_ORG)?
        .into_iter()
        .filter(|row| field(row, "owner_id") == scope.user_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "orgs": rows, "count": rows.len() }))
}

/// `GET /orgs/{org_id}`.
pub fn get_org(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)
}

/// `PATCH /orgs/{org_id}`, then the band the change earns.
pub fn update_org(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let mut patch = json!({});
    if let Some(object) = patch.as_object_mut() {
        if let Some(revenue) = optional(payload, "annual_revenue") {
            let amount =
                Money::parse(&revenue).map_err(|_| refused("annual_revenue is not an amount"))?;
            object.insert("annual_revenue".into(), wire(&amount));
        }
        if payload.get("headcount").is_some() {
            object.insert("headcount".into(), json!(whole(payload, "headcount")));
        }
        if let Some(sector) = optional(payload, "sector") {
            object.insert("sector".into(), json!(sector));
        }
    }

    let updated = Records::new(graph, NPO_SCALE_ORG).update(scope, org_id, &patch)?;
    // the band follows whatever the change did to revenue or headcount
    let band = refresh_band(graph, scope, &updated)?;
    let mut answer = updated;
    if let Some(object) = answer.as_object_mut() {
        object.insert("size_band".into(), json!(band));
    }
    let _ = org;
    Ok(answer)
}

/// `GET /orgs/{org_id}/features`: what this organisation type and band gives, and whether it needs a second approver.
pub fn org_features(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let band = field(&org, "size_band");
    let org_type = field(&org, "org_type");
    Ok(json!({
        "service": "npo-scale-service",
        "org_type": org_type.to_owned(),
        "size_band": band.to_owned(),
        "features": features_for(org_type, band),
        // nothing means no limit: a sole trader or a small organisation approves alone
        "approval_limit": approval_limit(band),
    }))
}

/// `POST /orgs/{org_id}/branches`, then the band the new branch earns.
pub fn create_branch(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let branch = remember(
        graph,
        scope,
        NPO_SCALE_BRANCH,
        json!({
            "org_id": org_id,
            "parent_id": optional(payload, "parent_id"),
            "name": text(payload, "name"),
            "region": text(payload, "region"),
            "status": "active",
        }),
    )?;
    // one more branch can be what tips an organisation into the next band
    let band = refresh_band(graph, scope, &org)?;
    Ok(json!({ "service": "npo-scale-service", "branch": branch, "size_band": band }))
}

/// `GET /orgs/{org_id}/branches`.
pub fn list_branches(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_BRANCH)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "branches": rows, "count": rows.len() }))
}

/// `POST /orgs/{org_id}/vendors`.
pub fn add_vendor(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let vendor = remember(
        graph,
        scope,
        NPO_SCALE_VENDOR,
        json!({
            "org_id": org_id,
            "name": name,
            "phone": text(payload, "phone"),
            "email": text(payload, "email"),
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "vendor": vendor }))
}

/// `GET /orgs/{org_id}/vendors`.
pub fn list_vendors(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_VENDOR)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "vendors": rows, "count": rows.len() }))
}

/// `POST /orgs/{org_id}/revenues`: money in, with the branch it belongs to when one is named.
pub fn record_revenue(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() || amount.is_negative() {
        return Err(refused("amount must be positive"));
    }
    // revenue entries are the commercial mirror of a donation, so a non-profit records donations instead
    let org_type = field(&org, "org_type");
    if !["commercial", "partnership", "company", "plc"].contains(&org_type) {
        return Err(refused(
            "Revenue entries are for commercial/partnership orgs",
        ));
    }

    let currency = or_default(payload, "currency", field(&org, "currency"));
    let received_at = text(payload, "received_at");
    let revenue = remember(
        graph,
        scope,
        NPO_SCALE_REVENUE,
        json!({
            "org_id": org_id,
            "branch_id": optional(payload, "branch_id"),
            "source": or_default(payload, "source", "sale"),
            "customer": text(payload, "customer"),
            "amount": wire(&amount),
            "currency": currency,
            "received_at": received_at,
        }),
    )?;
    // a sale is receipted like a donation: the customer gets something to check
    let receipt = issue_receipt(
        graph,
        scope,
        org_id,
        field(&revenue, "id"),
        Value::Null,
        &amount,
        &currency,
        &received_at,
    )?;
    Ok(json!({
        "service": "npo-scale-service",
        "revenue": revenue,
        "receipt_no": field(&receipt, "receipt_no"),
    }))
}

/// `GET /orgs/{org_id}/revenues`.
pub fn list_revenues(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_REVENUE)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    let total = Money::sum(
        &rows
            .iter()
            .map(|row| money_or_nothing(row, "amount"))
            .collect::<Result<Vec<Money>, StoreError>>()?,
    );
    Ok(json!({
        "service": "npo-scale-service",
        "revenues": rows,
        "count": rows.len(),
        "total": wire(&total),
    }))
}

/// `POST /orgs/{org_id}/purchases`: a purchase on credit, owed until it is paid.
pub fn record_purchase(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() || amount.is_negative() {
        return Err(refused("amount must be positive"));
    }
    let vendor_id = text(payload, "vendor_id");
    let known = rows_of(graph, scope, NPO_SCALE_VENDOR)?
        .iter()
        .any(|row| field(row, "id") == vendor_id && field(row, "org_id") == org_id);
    if !known {
        return Err(refused("Unknown vendor"));
    }

    let purchase = remember(
        graph,
        scope,
        NPO_SCALE_PURCHASE,
        json!({
            "org_id": org_id,
            "vendor_id": vendor_id,
            "description": text(payload, "description"),
            "amount": wire(&amount),
            "currency": or_default(payload, "currency", "USD"),
            "status": "unpaid",
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "purchase": purchase, "status": "unpaid" }))
}

/// `POST /orgs/{org_id}/purchases/{purchase_id}/pay`: settle it once, and book the expense it became.
pub fn pay_purchase(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    purchase_id: &str,
    payer: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let purchase = Records::new(graph, NPO_SCALE_PURCHASE).get(scope, purchase_id)?;
    if field(&purchase, "org_id") != org_id {
        return Err(StoreError::NotFound);
    }
    if field(&purchase, "status") == "paid" {
        return Err(refused("Already paid"));
    }

    Records::new(graph, NPO_SCALE_PURCHASE).update(
        scope,
        purchase_id,
        &json!({ "status": "paid" }),
    )?;

    // settling a purchase is spending: the expense is booked so every report stays honest
    remember(
        graph,
        scope,
        NPO_SCALE_EXPENSE,
        json!({
            "org_id": org_id,
            "branch_id": Value::Null,
            "fund": "general",
            "program": "",
            "functional_area": "operations",
            "amount": field(&purchase, "amount"),
            "currency": field(&purchase, "currency"),
            "description": field(&purchase, "description"),
            "approver1": payer,
            "approver2": Value::Null,
            "status": "approved",
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "status": "paid" }))
}

/// `GET /orgs/{org_id}/reports/creditors`: what is still owed right now, per vendor, largest first.
pub fn report_creditors(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let vendors = rows_of(graph, scope, NPO_SCALE_VENDOR)?;

    let mut owed: Vec<(String, Money)> = Vec::new();
    for purchase in rows_of(graph, scope, NPO_SCALE_PURCHASE)? {
        if field(&purchase, "org_id") != org_id || field(&purchase, "status") == "paid" {
            continue;
        }
        let vendor_id = field(&purchase, "vendor_id");
        let name = vendors
            .iter()
            .find(|vendor| field(vendor, "id") == vendor_id)
            .map(|vendor| field(vendor, "name"))
            .unwrap_or(vendor_id);
        let amount = money_or_nothing(&purchase, "amount")?;
        match owed.iter_mut().find(|(who, _)| *who == name) {
            Some((_, running)) => *running = running.plus(&amount),
            None => owed.push((name.to_owned(), amount)),
        }
    }

    // largest owed first, the order the report was read in
    owed.sort_by(|left, right| right.1.to_f64_lossy().total_cmp(&left.1.to_f64_lossy()));
    let total = Money::sum(
        &owed
            .iter()
            .map(|(_, amount)| amount.clone())
            .collect::<Vec<_>>(),
    );
    let creditors: Vec<Value> = owed
        .iter()
        .map(|(vendor, amount)| json!({ "vendor": vendor, "owed": wire(amount) }))
        .collect();

    Ok(json!({
        "service": "npo-scale-service",
        "creditors": creditors,
        "total_owed": wire(&total),
    }))
}

// ---------------------------------------------------------------------------
// Receipts: every donation and every sale gets one, numbered per organisation
// ---------------------------------------------------------------------------

/// The receipt number an organisation's next receipt gets: `RCP-<first eight of the org>-<five digits>`.
fn next_receipt_no(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<String, StoreError> {
    let issued = rows_of(graph, scope, NPO_SCALE_RECEIPT)?
        .iter()
        .filter(|row| field(row, "org_id") == org_id)
        .count();
    let prefix: String = org_id.chars().take(8).collect();
    Ok(format!("RCP-{prefix}-{:05}", issued + 1))
}

/// Issue a receipt against what it is for, and answer with it.
#[allow(clippy::too_many_arguments)]
fn issue_receipt(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    for_record: &str,
    donor_id: Value,
    amount: &Money,
    currency: &str,
    issued_at: &str,
) -> Result<Value, StoreError> {
    let receipt_no = next_receipt_no(graph, scope, org_id)?;
    let token = vimbai_common::ids::service_id();
    remember(
        graph,
        scope,
        NPO_SCALE_RECEIPT,
        json!({
            "org_id": org_id,
            "for_record": for_record,
            "donor_id": donor_id,
            "receipt_no": receipt_no,
            "amount": wire(amount),
            "currency": currency,
            "token": token,
            "issued_at": issued_at,
        }),
    )
}

/// `GET /orgs/{org_id}/receipts`.
pub fn list_scale_receipts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_RECEIPT)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "receipts": rows, "count": rows.len() }))
}

/// `GET /receipts/verify/{token}`: the public check a donor or an auditor runs.
///
/// No Book, because the person holding a receipt has no account here. Only what is printed on the receipt is
/// answered, so the token cannot be used to read the organisation's books.
pub fn verify_receipt(graph: &(impl Graph + ?Sized), token: &str) -> Result<Value, StoreError> {
    let receipt = Records::new(graph, NPO_SCALE_RECEIPT)
        .by_token("token", token)?
        .ok_or(StoreError::NotFound)?;

    // the organisation's name is what makes a receipt recognisable, so it is looked up by its own record
    let scope = BookScope::new(field(&receipt, "user_id"), field(&receipt, "book_id"));
    let org_name = Records::new(graph, NPO_SCALE_ORG)
        .get(&scope, field(&receipt, "org_id"))
        .map(|org| field(&org, "name").to_owned())
        .unwrap_or_default();

    Ok(json!({
        "service": "npo-scale-service",
        "valid": true,
        "receipt": {
            "receipt_no": field(&receipt, "receipt_no"),
            "amount": field(&receipt, "amount"),
            "currency": field(&receipt, "currency"),
            "issued_at": field(&receipt, "issued_at"),
            "name": org_name,
        },
    }))
}

/// The receipt as a real PDF, built here rather than by a dependency: a receipt a donor can keep or print.
pub fn receipt_pdf(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    receipt_id: &str,
) -> Result<Vec<u8>, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let receipt = Records::new(graph, NPO_SCALE_RECEIPT).get(scope, receipt_id)?;
    if field(&receipt, "org_id") != org_id {
        return Err(StoreError::NotFound);
    }
    Ok(pdf_of(field(&org, "name"), &receipt))
}

/// Make a string safe for a PDF literal.
fn pdf_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect()
}

fn pdf_of(org_name: &str, receipt: &Value) -> Vec<u8> {
    let issued = field(receipt, "issued_at")
        .get(..10)
        .unwrap_or("")
        .to_owned();
    let lines: [(u32, String); 11] = [
        (20, "OFFICIAL RECEIPT".into()),
        (14, org_name.to_owned()),
        (12, String::new()),
        (12, format!("Receipt No: {}", field(receipt, "receipt_no"))),
        (
            12,
            format!(
                "Amount: {} {}",
                field(receipt, "currency"),
                field(receipt, "amount")
            ),
        ),
        (12, format!("Date issued: {issued}")),
        (12, String::new()),
        (12, "Verify this receipt at:".into()),
        (10, format!("/receipts/verify/{}", field(receipt, "token"))),
        (12, String::new()),
        (10, format!("This receipt was issued by {org_name}.")),
    ];

    let mut content = String::from("BT\n50 770 Td\n14 TL\n");
    for (size, text) in lines {
        content.push_str(&format!("/F1 {size} Tf\n({}) Tj\nT*\n", pdf_escape(&text)));
    }
    content.push_str("ET");
    let stream = content.into_bytes();

    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        {
            let mut body = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
            body.extend_from_slice(&stream);
            body.extend_from_slice(b"\nendstream");
            body
        },
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    objects.retain(|object| !object.is_empty());

    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

// ---------------------------------------------------------------------------
// Donor CRM, donations, and the scheduled-giving collector
// ---------------------------------------------------------------------------

/// `POST /orgs/{org_id}/donors`.
pub fn create_donor(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let donor = remember(
        graph,
        scope,
        NPO_SCALE_DONOR,
        json!({
            "org_id": org_id,
            "name": name,
            "email": text(payload, "email"),
            "phone": text(payload, "phone"),
            "type": or_default(payload, "type", "individual"),
            "is_recurring": false,
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "donor": donor }))
}

/// `GET /orgs/{org_id}/donors`.
pub fn list_donors(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_DONOR)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "donors": rows, "count": rows.len() }))
}

/// `POST /orgs/{org_id}/donations`: the money in, and the receipt that proves it.
pub fn record_donation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let donor_id = text(payload, "donor_id");
    // a donation belongs to one of the organisation's own donors, not to any donor id a caller happens to know
    let known = rows_of(graph, scope, NPO_SCALE_DONOR)?
        .iter()
        .any(|row| field(row, "id") == donor_id && field(row, "org_id") == org_id);
    if !known {
        return Err(refused("Unknown donor for this org"));
    }
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() || amount.is_negative() {
        return Err(refused("amount must be positive"));
    }
    let currency = or_default(payload, "currency", "USD");
    let received_at = text(payload, "received_at");

    let donation = remember(
        graph,
        scope,
        NPO_SCALE_DONATION,
        json!({
            "org_id": org_id,
            "branch_id": optional(payload, "branch_id"),
            "donor_id": donor_id,
            "amount": wire(&amount),
            "currency": currency,
            "designation": or_default(payload, "designation", "general"),
            "source": or_default(payload, "source", "direct"),
            "received_at": received_at,
        }),
    )?;

    let receipt = issue_receipt(
        graph,
        scope,
        org_id,
        field(&donation, "id"),
        json!(donor_id),
        &amount,
        &currency,
        &received_at,
    )?;
    let receipt_id = field(&receipt, "id");
    Records::new(graph, NPO_SCALE_DONATION).update(
        scope,
        field(&donation, "id"),
        &json!({ "receipt_id": receipt_id }),
    )?;

    Ok(json!({
        "service": "npo-scale-service",
        "donation": donation,
        "receipt": {
            "receipt_no": field(&receipt, "receipt_no"),
            "token": field(&receipt, "token"),
            "amount": field(&receipt, "amount"),
            "currency": field(&receipt, "currency"),
        },
    }))
}

/// `GET /orgs/{org_id}/donations`, narrowed to one designation when one is named.
pub fn list_donations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    designation: Option<&str>,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_DONATION)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .filter(|row| match designation {
            Some(wanted) if !wanted.is_empty() => field(row, "designation") == wanted,
            _ => true,
        })
        .collect();
    Ok(json!({ "service": "npo-scale-service", "donations": rows, "count": rows.len() }))
}

/// `POST /orgs/{org_id}/pledges`: a recurring promise, first one due a month out.
pub fn create_pledge(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() || amount.is_negative() {
        return Err(refused("amount must be positive"));
    }
    // a due date is a calendar day, so the caller says which day this is rather than the crate asking a clock
    let today = text(payload, "today");
    let next_due = vimbai_common::dates::date_after(&today, 30).unwrap_or_default();

    let pledge = remember(
        graph,
        scope,
        NPO_SCALE_PLEDGE,
        json!({
            "org_id": org_id,
            "donor_id": text(payload, "donor_id"),
            "amount": wire(&amount),
            "currency": or_default(payload, "currency", "USD"),
            "frequency": or_default(payload, "frequency", "monthly"),
            "next_due": next_due,
            "status": "active",
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "pledge": pledge, "next_due": next_due }))
}

/// `POST /orgs/{org_id}/pledges/run`: collect every pledge that has come due, receipts and all.
pub fn run_pledges(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let today = text(payload, "today");
    let due: Vec<Value> = rows_of(graph, scope, NPO_SCALE_PLEDGE)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id && field(row, "status") == "active")
        .filter(|row| {
            let next_due = field(row, "next_due");
            !next_due.is_empty() && !today.is_empty() && next_due <= today.as_str()
        })
        .collect();

    let mut collected = 0;
    let mut total = Money::zero();
    for pledge in due {
        let amount = money_or_nothing(&pledge, "amount")?;
        let currency = field(&pledge, "currency");
        let donation = remember(
            graph,
            scope,
            NPO_SCALE_DONATION,
            json!({
                "org_id": org_id,
                "branch_id": Value::Null,
                "donor_id": field(&pledge, "donor_id"),
                "amount": wire(&amount),
                "currency": currency,
                "designation": "pledge",
                "source": "recurring",
                "received_at": today,
            }),
        )?;
        issue_receipt(
            graph,
            scope,
            org_id,
            field(&donation, "id"),
            json!(field(&pledge, "donor_id")),
            &amount,
            currency,
            &today,
        )?;
        // the next instalment is another month out, and the pledge stays active
        let next_due = vimbai_common::dates::date_after(&today, 30).unwrap_or_default();
        Records::new(graph, NPO_SCALE_PLEDGE).update(
            scope,
            field(&pledge, "id"),
            &json!({ "next_due": next_due }),
        )?;
        collected += 1;
        total = total.plus(&amount);
    }

    Ok(json!({
        "service": "npo-scale-service",
        "collected": collected,
        "total": wire(&total),
    }))
}

// ---------------------------------------------------------------------------
// Expenses: the size band decides whether one approver is enough
// ---------------------------------------------------------------------------

/// `POST /orgs/{org_id}/expenses`.
pub fn create_expense(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() || amount.is_negative() {
        return Err(refused("amount must be positive"));
    }

    let approver1 = text(payload, "approver1");
    let approver2 = optional(payload, "approver2").unwrap_or_default();
    let limit = approval_limit(field(&org, "size_band"));
    let over_limit = match limit {
        Some(ceiling) => amount.to_f64_lossy() > ceiling as f64,
        // no limit means the organisation approves alone
        None => false,
    };

    let mut status = "approved";
    if over_limit {
        if approver2.is_empty() {
            status = "pending_dual_approval";
        }
        if !approver2.is_empty() && approver2 == approver1 {
            return Err(refused("Dual approval requires two different approvers"));
        }
    }

    let expense = remember(
        graph,
        scope,
        NPO_SCALE_EXPENSE,
        json!({
            "org_id": org_id,
            "branch_id": optional(payload, "branch_id"),
            "fund": or_default(payload, "fund", "general"),
            "program": text(payload, "program"),
            "functional_area": or_default(payload, "functional_area", "program"),
            "amount": wire(&amount),
            "currency": or_default(payload, "currency", "USD"),
            "description": text(payload, "description"),
            "spent_at": text(payload, "spent_at"),
            "approver1": approver1,
            "approver2": approver2,
            "status": status,
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "expense": expense, "status": status }))
}

/// `GET /orgs/{org_id}/expenses`.
pub fn list_expenses(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_EXPENSE)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "expenses": rows, "count": rows.len() }))
}

// ---------------------------------------------------------------------------
// Budgets, compliance, and the balance items a position statement is made of
// ---------------------------------------------------------------------------

/// `PUT /orgs/{org_id}/budgets`: one budget line per fund and program, replaced rather than duplicated.
pub fn upsert_budget(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let budgeted = money_or_nothing(payload, "budgeted")?;
    let fiscal_year = whole(payload, "fiscal_year");
    let fund = or_default(payload, "fund", "general");
    let program = text(payload, "program");

    let existing = rows_of(graph, scope, NPO_SCALE_BUDGET)?
        .into_iter()
        .find(|row| {
            field(row, "org_id") == org_id
                && whole(row, "fiscal_year") == fiscal_year
                && field(row, "fund") == fund
                && field(row, "program") == program
        });

    // the same fund and program in the same year is the same line, however often it is saved
    match existing {
        Some(row) => {
            Records::new(graph, NPO_SCALE_BUDGET).update(
                scope,
                field(&row, "id"),
                &json!({ "budgeted": wire(&budgeted) }),
            )?;
        }
        None => {
            remember(
                graph,
                scope,
                NPO_SCALE_BUDGET,
                json!({
                    "org_id": org_id,
                    "fiscal_year": fiscal_year,
                    "fund": fund,
                    "program": program,
                    "budgeted": wire(&budgeted),
                }),
            )?;
        }
    }
    Ok(json!({ "service": "npo-scale-service", "status": "saved" }))
}

/// `POST /orgs/{org_id}/compliance`.
pub fn create_compliance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let item = remember(
        graph,
        scope,
        NPO_SCALE_COMPLIANCE,
        json!({
            "org_id": org_id,
            "title": text(payload, "title"),
            "category": or_default(payload, "category", "statutory"),
            "due_date": text(payload, "due_date"),
            "responsible": text(payload, "responsible"),
            "status": "open",
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "compliance": item }))
}

/// `GET /orgs/{org_id}/compliance`, soonest due first.
pub fn list_compliance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let mut rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_COMPLIANCE)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    rows.sort_by(|left, right| field(left, "due_date").cmp(field(right, "due_date")));
    Ok(json!({ "service": "npo-scale-service", "compliance": rows, "count": rows.len() }))
}

/// `POST /orgs/{org_id}/balance-items`: a thing the organisation owns or owes.
pub fn create_balance_item(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "kind");
    if !["asset", "liability"].contains(&kind.as_str()) {
        return Err(refused("kind must be asset or liability"));
    }
    held_org(graph, scope, org_id)?;
    let amount = money_or_nothing(payload, "amount")?;
    let item = remember(
        graph,
        scope,
        NPO_SCALE_BALANCE_ITEM,
        json!({
            "org_id": org_id,
            "kind": kind,
            "name": text(payload, "name"),
            "amount": wire(&amount),
            "currency": or_default(payload, "currency", "USD"),
            "as_of": text(payload, "as_of"),
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "item": item }))
}

// ---------------------------------------------------------------------------
// Private companies: shareholders, directors, dividends
// ---------------------------------------------------------------------------

/// The organisation, refused when it is not the kind this register is for.
fn org_of_kind(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    kinds: &[&str],
    refusal: &str,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let org_type = field(&org, "org_type");
    if !kinds.contains(&org_type) {
        return Err(refused(refusal));
    }
    Ok(org)
}

/// `POST /orgs/{org_id}/shareholders`: a holding, with a code its holder can check it by.
pub fn add_shareholder(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let shares = whole(payload, "shares");
    if shares < 1 {
        return Err(refused("shares must be >= 1"));
    }
    org_of_kind(
        graph,
        scope,
        org_id,
        &["company", "plc"],
        "Shareholders are for company/PLC orgs",
    )?;
    let capital = money_or_nothing(payload, "amount_paid")?;
    let verify_code = vimbai_common::ids::service_id().replace('-', "");
    let shareholder = remember(
        graph,
        scope,
        NPO_SCALE_SHAREHOLDER,
        json!({
            "org_id": org_id,
            "name": text(payload, "name"),
            "shares": shares,
            "share_class": or_default(payload, "share_class", "ordinary"),
            "amount_paid": wire(&capital),
            "verify_code": verify_code,
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "shareholder": shareholder }))
}

/// `GET /orgs/{org_id}/shareholders`.
pub fn list_shareholders(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_SHAREHOLDER)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "shareholders": rows, "count": rows.len() }))
}

/// `POST /orgs/{org_id}/directors`.
pub fn add_director(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    org_of_kind(
        graph,
        scope,
        org_id,
        &["company", "plc"],
        "Directors are for company/PLC orgs",
    )?;
    let director = remember(
        graph,
        scope,
        NPO_SCALE_DIRECTOR,
        json!({
            "org_id": org_id,
            "name": text(payload, "name"),
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "director": director }))
}

/// `GET /orgs/{org_id}/directors`.
pub fn list_directors(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_DIRECTOR)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "directors": rows, "count": rows.len() }))
}

/// What the organisation has earned and not yet distributed: revenue less expenses less dividends already declared.
fn distributable_reserves(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Money, StoreError> {
    let earned = total_of(rows_of(graph, scope, NPO_SCALE_REVENUE)?, org_id, "amount")?
        .minus(&total_of(
            rows_of(graph, scope, NPO_SCALE_EXPENSE)?,
            org_id,
            "amount",
        )?)
        .minus(&total_of(
            rows_of(graph, scope, NPO_SCALE_DIVIDEND)?,
            org_id,
            "total",
        )?);
    Ok(earned)
}

/// `POST /orgs/{org_id}/dividends`: a dividend per share, capped at what has been earned and not yet paid out.
pub fn declare_dividend(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    org_of_kind(
        graph,
        scope,
        org_id,
        &["company", "plc"],
        "Dividends are for company/PLC orgs",
    )?;
    let per_share = money_or_nothing(payload, "per_share")?;
    if per_share.is_zero() || per_share.is_negative() {
        return Err(refused("per_share must be positive"));
    }

    let shares: u64 = rows_of(graph, scope, NPO_SCALE_SHAREHOLDER)?
        .iter()
        .filter(|row| field(row, "org_id") == org_id)
        .map(|row| whole(row, "shares") as u64)
        .sum();
    if shares == 0 {
        return Err(refused("Add shareholders before declaring dividends"));
    }

    // a dividend is paid out of what the company has earned: an unpaid one is not reserves
    let reserves = distributable_reserves(graph, scope, org_id)?;
    let total = per_share.times(&Money::from_u64(shares));
    if !total.minus(&reserves).is_negative() && !total.minus(&reserves).is_zero() {
        return Err(refused(&format!(
            "Dividend exceeds distributable reserves ({})",
            reserves.to_fixed(2)
        )));
    }

    let dividend = remember(
        graph,
        scope,
        NPO_SCALE_DIVIDEND,
        json!({
            "org_id": org_id,
            "per_share": wire(&per_share),
            "total": wire(&total),
            "status": "declared",
        }),
    )?;
    Ok(json!({
        "service": "npo-scale-service",
        "dividend": dividend,
        "total_shares": shares,
        "total": wire(&total),
    }))
}

/// `GET /orgs/{org_id}/reports/equity`: share capital paid in, retained earnings, dividends, and the closing figure.
pub fn report_equity(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    org_of_kind(
        graph,
        scope,
        org_id,
        &["company", "plc"],
        "Equity statement is for company/PLC orgs",
    )?;
    let capital = total_of(
        rows_of(graph, scope, NPO_SCALE_SHAREHOLDER)?,
        org_id,
        "amount_paid",
    )?;
    let earned = total_of(rows_of(graph, scope, NPO_SCALE_REVENUE)?, org_id, "amount")?.minus(
        &total_of(rows_of(graph, scope, NPO_SCALE_EXPENSE)?, org_id, "amount")?,
    );
    let dividends = total_of(rows_of(graph, scope, NPO_SCALE_DIVIDEND)?, org_id, "total")?;
    let retained = earned.minus(&dividends);

    Ok(json!({
        "service": "npo-scale-service",
        "share_capital": wire(&capital),
        "retained_earnings": wire(&retained),
        "dividends_declared": wire(&dividends),
        "total_equity": wire(&capital.plus(&retained)),
    }))
}

/// `GET /public/holdings/{verify_code}`: an investor checks their own holding without seeing the books.
pub fn public_holdings(
    graph: &(impl Graph + ?Sized),
    verify_code: &str,
) -> Result<Value, StoreError> {
    let holding = Records::new(graph, NPO_SCALE_SHAREHOLDER)
        .by_token("verify_code", verify_code)?
        .ok_or(StoreError::NotFound)?;
    let scope = BookScope::new(field(&holding, "user_id"), field(&holding, "book_id"));
    let org_id = field(&holding, "org_id");
    let org_name = Records::new(graph, NPO_SCALE_ORG)
        .get(&scope, org_id)
        .map(|org| field(&org, "name").to_owned())
        .unwrap_or_default();

    let shares = whole(&holding, "shares") as u64;
    let total_shares: u64 = rows_of(graph, &scope, NPO_SCALE_SHAREHOLDER)?
        .iter()
        .filter(|row| field(row, "org_id") == org_id)
        .map(|row| whole(row, "shares") as u64)
        .sum();

    let mut due = Money::zero();
    let mut dividends = Vec::new();
    for declared in rows_of(graph, &scope, NPO_SCALE_DIVIDEND)? {
        if field(&declared, "org_id") != org_id {
            continue;
        }
        let per_share = money_or_nothing(&declared, "per_share")?;
        let amount = per_share.times(&Money::from_u64(shares));
        due = due.plus(&amount);
        dividends.push(json!({
            "per_share": wire(&per_share),
            "amount": wire(&amount),
            "declared_at": field(&declared, "created_at"),
        }));
    }

    // the holding as a percentage of the company, to two places, exactly
    let percentage = if total_shares == 0 {
        Money::zero()
    } else {
        Money::from_u64(shares).ratio_pct(&Money::from_u64(total_shares), 2)
    };

    Ok(json!({
        "service": "npo-scale-service",
        "valid": true,
        "org_name": org_name,
        "shareholder": field(&holding, "name"),
        "shares": shares,
        "share_class": field(&holding, "share_class"),
        "capital_paid": field(&holding, "amount_paid"),
        "percentage": wire(&percentage),
        "total_dividends": wire(&due),
        "dividends": dividends,
    }))
}

// ---------------------------------------------------------------------------
// Partnerships: partners, their draws, and their capital accounts
// ---------------------------------------------------------------------------

/// `POST /orgs/{org_id}/partners`.
pub fn add_scale_partner(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    org_of_kind(
        graph,
        scope,
        org_id,
        &["partnership"],
        "Partners can only be added to partnership orgs",
    )?;
    let contribution = money_or_nothing(payload, "capital_contribution")?;
    let share = money_or_nothing(payload, "profit_share")?;
    let above_a_hundred = share.minus(&Money::from_i64(100));
    if share.is_negative() || (!above_a_hundred.is_negative() && !above_a_hundred.is_zero()) {
        return Err(refused("profit_share must be 0-100"));
    }
    let partner = remember(
        graph,
        scope,
        NPO_SCALE_PARTNER,
        json!({
            "org_id": org_id,
            "name": text(payload, "name"),
            "capital_contribution": wire(&contribution),
            "profit_share": wire(&share),
            "status": "active",
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "partner": partner }))
}

/// `GET /orgs/{org_id}/partners`.
pub fn list_partners(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let rows: Vec<Value> = rows_of(graph, scope, NPO_SCALE_PARTNER)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id)
        .collect();
    Ok(json!({ "service": "npo-scale-service", "partners": rows, "count": rows.len() }))
}

/// `POST /orgs/{org_id}/partners/{partner_id}/draws`.
pub fn partner_draw(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    partner_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let amount = money_or_nothing(payload, "amount")?;
    if amount.is_zero() || amount.is_negative() {
        return Err(refused("amount must be positive"));
    }
    let partner = Records::new(graph, NPO_SCALE_PARTNER).get(scope, partner_id)?;
    if field(&partner, "org_id") != org_id {
        return Err(refused("Unknown partner"));
    }
    remember(
        graph,
        scope,
        NPO_SCALE_PARTNER_DRAW,
        json!({
            "org_id": org_id,
            "partner_id": partner_id,
            "amount": wire(&amount),
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "status": "drawn" }))
}

/// `GET /orgs/{org_id}/reports/capital-accounts`: contribution plus share of income less draws, per partner.
pub fn report_capital_accounts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    org_of_kind(
        graph,
        scope,
        org_id,
        &["partnership"],
        "Capital accounts are for partnership orgs",
    )?;
    // a partnership's income is everything it took in, donations included
    let income = total_of(rows_of(graph, scope, NPO_SCALE_REVENUE)?, org_id, "amount")?
        .plus(&total_of(
            rows_of(graph, scope, NPO_SCALE_DONATION)?,
            org_id,
            "amount",
        )?)
        .minus(&total_of(
            rows_of(graph, scope, NPO_SCALE_EXPENSE)?,
            org_id,
            "amount",
        )?);

    let partners: Vec<Value> = rows_of(graph, scope, NPO_SCALE_PARTNER)?
        .into_iter()
        .filter(|row| field(row, "org_id") == org_id && field(row, "status") == "active")
        .collect();
    let shares: Vec<Money> = partners
        .iter()
        .map(|partner| money_or_nothing(partner, "profit_share"))
        .collect::<Result<_, _>>()?;
    // shares that do not add to 100 are allocated in proportion, so a partnership is never left with income unassigned
    let total_share = Money::sum(&shares);
    let divisor = if total_share.is_zero() {
        Money::from_i64(1)
    } else {
        total_share
    };

    let mut accounts = Vec::new();
    for (partner, share) in partners.iter().zip(shares.iter()) {
        let partner_id = field(partner, "id");
        let draws = Money::sum(
            &rows_of(graph, scope, NPO_SCALE_PARTNER_DRAW)?
                .iter()
                .filter(|row| field(row, "partner_id") == partner_id)
                .map(|row| money_or_nothing(row, "amount"))
                .collect::<Result<Vec<Money>, StoreError>>()?,
        );
        let contribution = money_or_nothing(partner, "capital_contribution")?;
        // the share of income is the partner's percentage of the shares actually held, to two places
        let allocated = income.times(share).over(&divisor).round(2);
        accounts.push(json!({
            "partner": field(partner, "name"),
            "contribution": wire(&contribution),
            "profit_share_pct": wire(share),
            "allocated_income": wire(&allocated),
            "draws": wire(&draws),
            "capital_account": wire(&contribution.plus(&allocated).minus(&draws)),
        }));
    }

    Ok(json!({
        "service": "npo-scale-service",
        "net_income": wire(&income),
        "accounts": accounts,
    }))
}

// ---------------------------------------------------------------------------
// The five reports
// ---------------------------------------------------------------------------

/// Everything one organisation recorded under a property, added up.
fn total_of(rows: Vec<Value>, org_id: &str, property: &str) -> Result<Money, StoreError> {
    let amounts: Vec<Money> = rows
        .iter()
        .filter(|row| field(row, "org_id") == org_id)
        .map(|row| money_or_nothing(row, property))
        .collect::<Result<_, _>>()?;
    Ok(Money::sum(&amounts))
}

/// `GET /orgs/{org_id}/reports/activities`: revenue by fund less expenses by fund, with the organisation's net.
///
/// A non-profit's revenue is what it was given, so donations are read; every other organisation's is what it
/// earned, so its sales are.
pub fn report_activities(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let org_type = field(&org, "org_type");
    let commercial = ["commercial", "partnership", "company", "plc"].contains(&org_type);

    let (source, revenue_property) = if commercial {
        (NPO_SCALE_REVENUE, "source")
    } else {
        (NPO_SCALE_DONATION, "designation")
    };

    let mut funds: Vec<(String, Money, Money)> = Vec::new();
    let mut note = |fund: &str, revenue: Option<&Money>, expense: Option<&Money>| {
        let position = match funds.iter().position(|(name, _, _)| name == fund) {
            Some(position) => position,
            None => {
                funds.push((fund.to_owned(), Money::zero(), Money::zero()));
                funds.len() - 1
            }
        };
        if let Some(amount) = revenue {
            funds[position].1 = funds[position].1.plus(amount);
        }
        if let Some(amount) = expense {
            funds[position].2 = funds[position].2.plus(amount);
        }
    };

    for row in rows_of(graph, scope, source)? {
        if field(&row, "org_id") != org_id {
            continue;
        }
        let amount = money_or_nothing(&row, "amount")?;
        note(field(&row, revenue_property), Some(&amount), None);
    }
    for row in rows_of(graph, scope, NPO_SCALE_EXPENSE)? {
        if field(&row, "org_id") != org_id {
            continue;
        }
        let amount = money_or_nothing(&row, "amount")?;
        note(field(&row, "fund"), None, Some(&amount));
    }

    let mut total = Money::zero();
    let lines: Vec<Value> = funds
        .iter()
        .map(|(fund, revenue, expenses)| {
            let net = revenue.minus(expenses);
            total = total.plus(&net);
            json!({
                "fund": fund,
                "revenue": wire(revenue),
                "expenses": wire(expenses),
                "net": wire(&net),
            })
        })
        .collect();

    Ok(json!({
        "service": "npo-scale-service",
        "funds": lines,
        "total_net": wire(&total),
    }))
}

/// `GET /orgs/{org_id}/reports/functional-expenses`: program against administration against fundraising.
pub fn report_functional(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let mut areas: Vec<(String, Money)> = Vec::new();
    for row in rows_of(graph, scope, NPO_SCALE_EXPENSE)? {
        if field(&row, "org_id") != org_id {
            continue;
        }
        let area = field(&row, "functional_area").to_owned();
        let amount = money_or_nothing(&row, "amount")?;
        match areas.iter_mut().find(|(name, _)| *name == area) {
            Some((_, running)) => *running = running.plus(&amount),
            None => areas.push((area, amount)),
        }
    }
    let total = Money::sum(
        &areas
            .iter()
            .map(|(_, amount)| amount.clone())
            .collect::<Vec<_>>(),
    );
    let program = areas
        .iter()
        .find(|(name, _)| name == "program")
        .map(|(_, amount)| amount.clone())
        .unwrap_or_else(Money::zero);
    // the share of spending that reached the programme, to four places, and nothing when nothing was spent
    let ratio = if total.is_zero() {
        Money::zero()
    } else {
        program.over(&total).round(6)
    };

    Ok(json!({
        "service": "npo-scale-service",
        "areas": areas
            .iter()
            .map(|(area, amount)| json!({ "functional_area": area, "total": wire(amount) }))
            .collect::<Vec<Value>>(),
        "total": wire(&total),
        // the shipped answer is a fraction, so 0.75 means three quarters of the spending was programme work
        "program_ratio": wire(&ratio),
    }))
}

/// `GET /orgs/{org_id}/reports/position`: what it owns, what it owes, and what is left.
pub fn report_position(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let items = rows_of(graph, scope, NPO_SCALE_BALANCE_ITEM)?;
    let side = |kind: &str| -> Result<Money, StoreError> {
        let amounts: Vec<Money> = items
            .iter()
            .filter(|row| field(row, "org_id") == org_id && field(row, "kind") == kind)
            .map(|row| money_or_nothing(row, "amount"))
            .collect::<Result<_, _>>()?;
        Ok(Money::sum(&amounts))
    };
    let assets = side("asset")?;
    let liabilities = side("liability")?;

    // the cash position stands in for the balance sheet: what came in, less what went out
    let taken_in = total_of(rows_of(graph, scope, NPO_SCALE_DONATION)?, org_id, "amount")?.plus(
        &total_of(rows_of(graph, scope, NPO_SCALE_REVENUE)?, org_id, "amount")?,
    );
    let spent = total_of(rows_of(graph, scope, NPO_SCALE_EXPENSE)?, org_id, "amount")?;
    let net_assets = assets.plus(&taken_in).minus(&spent).minus(&liabilities);

    Ok(json!({
        "service": "npo-scale-service",
        "assets": wire(&assets),
        "liabilities": wire(&liabilities),
        "net_assets": wire(&net_assets),
        "movement": wire(&taken_in.minus(&spent)),
    }))
}

/// `GET /orgs/{org_id}/reports/budget-vs-actual`: each budget line against what was actually spent on it.
pub fn report_budget(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
    fiscal_year: Option<u32>,
) -> Result<Value, StoreError> {
    held_org(graph, scope, org_id)?;
    let expenses = rows_of(graph, scope, NPO_SCALE_EXPENSE)?;

    let mut lines = Vec::new();
    for budget in rows_of(graph, scope, NPO_SCALE_BUDGET)? {
        if field(&budget, "org_id") != org_id {
            continue;
        }
        if let Some(year) = fiscal_year {
            if whole(&budget, "fiscal_year") != year {
                continue;
            }
        }
        let fund = field(&budget, "fund");
        let program = field(&budget, "program");
        let spent: Vec<Money> = expenses
            .iter()
            .filter(|row| field(row, "org_id") == org_id)
            .filter(|row| field(row, "fund") == fund && field(row, "program") == program)
            .map(|row| money_or_nothing(row, "amount"))
            .collect::<Result<_, _>>()?;
        let actual = Money::sum(&spent);
        let budgeted = money_or_nothing(&budget, "budgeted")?;
        // how much of the budget is used, to four places, and nothing at all when nothing was budgeted
        let utilization = if budgeted.is_zero() {
            Money::zero()
        } else {
            actual.over(&budgeted).round(4)
        };
        lines.push(json!({
            "fund": fund,
            "program": program,
            "budgeted": wire(&budgeted),
            "actual": wire(&actual),
            "variance": wire(&budgeted.minus(&actual)),
            "utilization": wire(&utilization),
        }));
    }

    Ok(json!({ "service": "npo-scale-service", "lines": lines }))
}

/// `GET /orgs/{org_id}/reports/consolidated`: the federation view of every branch, once an organisation is large.
pub fn report_consolidated(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    org_id: &str,
) -> Result<Value, StoreError> {
    let org = held_org(graph, scope, org_id)?;
    let band = field(&org, "size_band");
    // consolidating a federation is a large organisation's report
    if !["large", "extra_large"].contains(&band) {
        return Err(refused("Consolidated reporting unlocks at large scale"));
    }

    let donations = rows_of(graph, scope, NPO_SCALE_DONATION)?;
    let expenses = rows_of(graph, scope, NPO_SCALE_EXPENSE)?;
    let side = |rows: &[Value], branch: Option<&str>| -> Result<Money, StoreError> {
        let amounts: Vec<Money> = rows
            .iter()
            .filter(|row| field(row, "org_id") == org_id)
            .filter(|row| branch_of(row) == branch.map(str::to_owned))
            .map(|row| money_or_nothing(row, "amount"))
            .collect::<Result<_, _>>()?;
        Ok(Money::sum(&amounts))
    };

    let mut branches = Vec::new();
    for branch in rows_of(graph, scope, NPO_SCALE_BRANCH)? {
        if field(&branch, "org_id") != org_id {
            continue;
        }
        let id = field(&branch, "id");
        let given = side(&donations, Some(id))?;
        let spent = side(&expenses, Some(id))?;
        branches.push(json!({
            "branch": field(&branch, "name"),
            "donations": wire(&given),
            "expenses": wire(&spent),
            "net": wire(&given.minus(&spent)),
        }));
    }

    // what was booked at head office belongs to no branch, and is still the organisation's
    let hq_given = side(&donations, None)?;
    let hq_spent = side(&expenses, None)?;
    let total_given = Money::sum(
        &branches
            .iter()
            .map(|branch| {
                Money::parse(field(branch, "donations")).unwrap_or_else(|_| Money::zero())
            })
            .collect::<Vec<Money>>(),
    )
    .plus(&hq_given);
    let total_spent = Money::sum(
        &branches
            .iter()
            .map(|branch| Money::parse(field(branch, "expenses")).unwrap_or_else(|_| Money::zero()))
            .collect::<Vec<Money>>(),
    )
    .plus(&hq_spent);

    Ok(json!({
        "service": "npo-scale-service",
        "size_band": band,
        "hq": { "donations": wire(&hq_given), "expenses": wire(&hq_spent) },
        "branches": branches,
        "consolidated": {
            "donations": wire(&total_given),
            "expenses": wire(&total_spent),
            "net": wire(&total_given.minus(&total_spent)),
        },
    }))
}

/// The branch a record was booked to, if any.
fn branch_of(row: &Value) -> Option<String> {
    match row.get("branch_id") {
        Some(Value::String(branch)) if !branch.is_empty() => Some(branch.clone()),
        _ => None,
    }
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
    fn a_receipt_is_numbered_from_the_first_and_counts_per_organisation() {
        let graph = FakeGraph::queue(vec![
            Vec::new(),
            vec![json!({"id": "r-1", "org_id": "abcdefgh-1"})],
        ]);
        let receipt = issue_receipt(
            &graph,
            &scope(),
            "abcdefgh-1",
            "donation-1",
            Value::Null,
            &Money::from_i64(250),
            "USD",
            "2026-03-01",
        )
        .expect("issued");
        assert_eq!(receipt["receipt_no"], json!("RCP-abcdefgh-00001"));
        // a token is minted, and it is what a donor hands back to check the receipt
        assert!(!field(&receipt, "token").is_empty());
    }

    #[test]
    fn a_donation_for_a_donor_of_another_organisation_is_refused() {
        let graph = FakeGraph::queue(vec![
            vec![json!({
                "id": "org-1",
                "name": "Harare Youth Trust",
                "org_type": "nonprofit",
                "book_id": "book-1",
                "user_id": "user-1",
                "owner_id": "user-1",
            })],
            vec![json!({
                "id": "donor-9",
                "org_id": "another-org",
                "name": "Elsewhere",
            })],
        ]);
        let refused = record_donation(
            &graph,
            &scope(),
            "org-1",
            &json!({"donor_id": "donor-9", "amount": 100}),
        )
        .expect_err("refused");
        assert!(refused.to_string().contains("Unknown donor for this org"));
    }

    #[test]
    fn a_dividend_cannot_exceed_what_the_company_has_earned() {
        let org = json!({
            "id": "org-1",
            "name": "Kaguvi Holdings",
            "org_type": "company",
            "book_id": "book-1",
            "user_id": "user-1",
            "owner_id": "user-1",
        });
        let graph = FakeGraph::queue(vec![
            vec![org.clone()],
            // two shareholders holding 1000 shares between them
            vec![
                json!({"id": "s-1", "org_id": "org-1", "shares": 600, "amount_paid": "600.000000"}),
                json!({"id": "s-2", "org_id": "org-1", "shares": 400, "amount_paid": "400.000000"}),
            ],
            // earned 5,000 on sales
            vec![json!({"id": "r-1", "org_id": "org-1", "amount": "5000.000000"})],
            Vec::new(),
            Vec::new(),
            vec![
                json!({"id": "d-1", "org_id": "org-1", "per_share": "4.000000", "total": "4000.000000"}),
            ],
        ]);
        let declared = declare_dividend(&graph, &scope(), "org-1", &json!({"per_share": "4.00"}))
            .expect("declared");
        assert_eq!(declared["total_shares"], json!(1000));
        assert_eq!(declared["total"], json!("4000.000000"));

        // 6.00 a share is 6,000 against 5,000 earned, so there is nothing to pay it from
        let graph = FakeGraph::queue(vec![
            vec![org],
            vec![
                json!({"id": "s-1", "org_id": "org-1", "shares": 1000, "amount_paid": "1000.000000"}),
            ],
            vec![json!({"id": "r-1", "org_id": "org-1", "amount": "5000.000000"})],
            Vec::new(),
            Vec::new(),
        ]);
        let refused = declare_dividend(&graph, &scope(), "org-1", &json!({"per_share": "6.00"}))
            .expect_err("refused");
        assert!(refused.to_string().contains("distributable reserves"));
    }

    #[test]
    fn a_budget_line_is_replaced_rather_than_duplicated() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "org-1", "name": "T", "org_type": "nonprofit", "book_id": "book-1", "user_id": "user-1", "owner_id": "user-1"}),
            ],
            vec![json!({
                "id": "budget-1",
                "org_id": "org-1",
                "fiscal_year": 2026,
                "fund": "general",
                "program": "clinics",
                "budgeted": "1000.000000",
            })],
            vec![json!({"id": "budget-1", "budgeted": "2500.000000"})],
        ]);
        upsert_budget(
            &graph,
            &scope(),
            "org-1",
            &json!({"fiscal_year": 2026, "fund": "general", "program": "clinics", "budgeted": "2500"}),
        )
        .expect("saved");
        // the same fund and program in the same year is the same line, so nothing new was written
        let (statement, params) = graph.statement(2);
        assert!(statement.contains("SET x += $patch"), "{statement}");
        assert_eq!(params["patch"]["budgeted"], json!("2500.000000"));
    }

    #[test]
    fn an_expense_over_the_band_limit_needs_a_second_approver() {
        let org = json!({
            "id": "org-1",
            "name": "Harare Youth Trust",
            "org_type": "nonprofit",
            "size_band": "medium",
            "book_id": "book-1",
            "user_id": "user-1",
            "owner_id": "user-1",
        });
        let graph = FakeGraph::queue(vec![
            vec![org.clone()],
            vec![json!({"id": "expense-1", "status": "pending_dual_approval"})],
        ]);
        let answer = create_expense(
            &graph,
            &scope(),
            "org-1",
            &json!({"amount": "20000.00", "approver1": "tendai"}),
        )
        .expect("recorded");
        assert_eq!(answer["status"], json!("pending_dual_approval"));

        // the same approval twice is not two approvals
        let graph = FakeGraph::queue(vec![vec![org]]);
        let refused = create_expense(
            &graph,
            &scope(),
            "org-1",
            &json!({"amount": "20000.00", "approver1": "tendai", "approver2": "tendai"}),
        )
        .expect_err("refused");
        assert!(refused.to_string().contains("two different approvers"));
    }

    #[test]
    fn the_consolidated_report_is_a_large_organisations_report() {
        let graph = FakeGraph::queue(vec![vec![json!({
            "id": "org-1",
            "name": "Zimbabwe Widows Fund",
            "org_type": "nonprofit",
            "size_band": "medium",
            "book_id": "book-1",
            "user_id": "user-1",
            "owner_id": "user-1",
        })]]);
        let refused = report_consolidated(&graph, &scope(), "org-1").expect_err("refused");
        assert!(refused.to_string().contains("large scale"));
    }

    #[test]
    fn a_receipt_pdf_is_a_document_a_reader_will_accept() {
        let pdf = pdf_of(
            "Harare Youth Trust",
            &json!({
                "receipt_no": "RCP-abcdefgh-00001",
                "amount": "250.000000",
                "currency": "USD",
                "issued_at": "2026-03-01",
                "token": "abc123",
            }),
        );
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"), "not a PDF header");
        assert!(text.trim_end().ends_with("%%EOF"), "no end of file");
        assert!(text.contains("OFFICIAL RECEIPT"));
        assert!(text.contains("RCP-abcdefgh-00001"));
        assert!(text.contains("/receipts/verify/abc123"));
        // the cross-reference table points at the five objects the document declares
        assert!(text.contains("/Size 6"));
    }

    #[test]
    fn a_partners_share_of_income_is_proportional_when_the_shares_do_not_add_to_a_hundred() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "org-1", "name": "Mhondoro Partners", "org_type": "partnership", "book_id": "book-1", "user_id": "user-1", "owner_id": "user-1"}),
            ],
            // 3,000 earned
            vec![json!({"id": "r-1", "org_id": "org-1", "amount": "3000.000000"})],
            Vec::new(),
            Vec::new(),
            vec![
                json!({"id": "p-1", "org_id": "org-1", "name": "Rudo", "capital_contribution": "1000.000000", "profit_share": "60.000000", "status": "active"}),
                json!({"id": "p-2", "org_id": "org-1", "name": "Farai", "capital_contribution": "1000.000000", "profit_share": "60.000000", "status": "active"}),
            ],
            // no draws
            Vec::new(),
            Vec::new(),
        ]);
        let answer = report_capital_accounts(&graph, &scope(), "org-1").expect("reported");
        // two partners holding 60 each is 120 between them, so each is entitled to half of the income
        let accounts = answer["accounts"].as_array().expect("accounts");
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0]["allocated_income"], json!("1500.000000"));
        assert_eq!(accounts[0]["capital_account"], json!("2500.000000"));
        assert_eq!(answer["net_income"], json!("3000.000000"));
    }

    #[test]
    fn the_size_band_follows_the_revenue() {
        assert_eq!(band_for(0, 0, 0, "nonprofit"), "small");
        assert_eq!(band_for(49_999, 0, 0, "nonprofit"), "small");
        assert_eq!(band_for(50_000, 0, 0, "nonprofit"), "medium");
        assert_eq!(band_for(500_000, 0, 0, "nonprofit"), "large");
        assert_eq!(band_for(5_000_000, 0, 0, "nonprofit"), "extra_large");
    }

    #[test]
    fn a_company_starts_higher_than_a_non_profit() {
        // 100k is medium for a non-profit but still small for a company
        assert_eq!(band_for(100_000, 0, 0, "nonprofit"), "medium");
        assert_eq!(band_for(100_000, 0, 0, "company"), "small");
        assert_eq!(band_for(600_000, 0, 0, "company"), "medium");
    }

    #[test]
    fn branches_or_people_push_an_organisation_up_a_band() {
        assert_eq!(band_for(0, 0, 6, "nonprofit"), "large");
        assert_eq!(band_for(0, 51, 0, "nonprofit"), "large");
        assert_eq!(band_for(0, 600, 0, "nonprofit"), "extra_large");
        assert_eq!(band_for(0, 0, 26, "nonprofit"), "extra_large");
        // a large organisation is never pushed back down
        assert_eq!(band_for(5_000_000, 0, 6, "nonprofit"), "extra_large");
    }

    #[test]
    fn an_organisation_is_created_at_its_own_band_with_the_caller_as_owner() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "org-1"})]]);
        create_org(
            &graph,
            &scope(),
            &json!({"name": "Harare Youth Trust", "annual_revenue": 60000, "size_band": "extra_large"}),
        )
        .expect("created");

        let written = props(&graph, 0);
        assert_eq!(written["owner_id"], json!("user-1"));
        assert_eq!(written["size_band"], json!("medium"));
        assert_eq!(written["org_type"], json!("nonprofit"));
        assert_eq!(written["country"], json!("ZW"));
        assert_eq!(written["annual_revenue"], json!("60000.000000"));
    }

    #[test]
    fn an_unknown_organisation_type_is_refused_by_name() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_org(
            &graph,
            &scope(),
            &json!({"name": "X", "org_type": "charity"}),
        );
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("org_type must be nonprofit"));
    }

    #[test]
    fn another_tenants_organisation_is_a_miss_that_looks_like_no_record() {
        let graph = FakeGraph::returning(vec![json!({"id": "org-1", "owner_id": "user-2"})]);
        let miss = get_org(&graph, &scope(), "org-1");
        assert!(miss.is_err());
    }

    #[test]
    fn the_features_follow_the_type_and_the_band() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "org-1",
            "owner_id": "user-1",
            "org_type": "company",
            "size_band": "small",
        })]);
        let answer = org_features(&graph, &scope(), "org-1").expect("answered");
        let features = answer["features"].as_array().unwrap();
        assert!(features.iter().any(|f| f == "share_capital"));
        // a small company approves alone
        assert_eq!(answer["approval_limit"], Value::Null);
    }

    #[test]
    fn a_medium_organisation_needs_a_second_approver() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "org-1",
            "owner_id": "user-1",
            "org_type": "nonprofit",
            "size_band": "medium",
        })]);
        let answer = org_features(&graph, &scope(), "org-1").expect("answered");
        assert_eq!(answer["approval_limit"], json!(5000));
    }

    #[test]
    fn a_purchase_settles_once_and_books_its_expense() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
            ],
            vec![
                json!({"id": "p-1", "org_id": "org-1", "status": "unpaid", "amount": "120.00", "currency": "USD", "description": "paper"}),
            ],
            vec![json!({"id": "p-1"})],
            vec![json!({"id": "e-1"})],
        ]);
        pay_purchase(&graph, &scope(), "org-1", "p-1", "user-1").expect("paid");

        assert_eq!(props(&graph, 2)["status"], json!("paid"));
        let expense = props(&graph, 3);
        assert_eq!(expense["amount"], json!("120.00"));
        assert_eq!(expense["functional_area"], json!("operations"));
        assert_eq!(expense["approver1"], json!("user-1"));
        assert_eq!(expense["status"], json!("approved"));
    }

    #[test]
    fn a_purchase_already_paid_is_refused() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
            ],
            vec![json!({"id": "p-1", "org_id": "org-1", "status": "paid", "amount": "120.00"})],
        ]);
        let refused = pay_purchase(&graph, &scope(), "org-1", "p-1", "user-1");
        assert!(refused.unwrap_err().to_string().contains("Already paid"));
    }

    #[test]
    fn a_purchase_for_an_unknown_vendor_is_refused() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
            ],
            vec![],
        ]);
        let refused = record_purchase(
            &graph,
            &scope(),
            "org-1",
            &json!({"vendor_id": "v-9", "amount": 50}),
        );
        assert!(refused.unwrap_err().to_string().contains("Unknown vendor"));
    }

    #[test]
    fn the_creditors_report_is_what_is_still_owed_largest_first() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small"}),
            ],
            vec![
                json!({"id": "v-1", "org_id": "org-1", "name": "Stationery"}),
                json!({"id": "v-2", "org_id": "org-1", "name": "Fuel"}),
            ],
            vec![
                json!({"id": "p-1", "org_id": "org-1", "vendor_id": "v-1", "status": "unpaid", "amount": "80.00"}),
                json!({"id": "p-2", "org_id": "org-1", "vendor_id": "v-2", "status": "unpaid", "amount": "300.00"}),
                json!({"id": "p-3", "org_id": "org-1", "vendor_id": "v-1", "status": "unpaid", "amount": "40.00"}),
                json!({"id": "p-4", "org_id": "org-1", "vendor_id": "v-2", "status": "paid", "amount": "999.00"}),
            ],
        ]);
        let answer = report_creditors(&graph, &scope(), "org-1").expect("reported");

        let creditors = answer["creditors"].as_array().unwrap();
        assert_eq!(creditors.len(), 2);
        // fuel is owed 300, stationery 120 together; the paid purchase is not owed at all
        assert_eq!(creditors[0]["vendor"], json!("Fuel"));
        assert_eq!(creditors[0]["owed"], json!("300.000000"));
        assert_eq!(creditors[1]["vendor"], json!("Stationery"));
        assert_eq!(creditors[1]["owed"], json!("120.000000"));
        assert_eq!(answer["total_owed"], json!("420.000000"));
    }

    #[test]
    fn a_branch_tips_the_organisation_into_the_next_band() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small", "annual_revenue": "0", "headcount": 0}),
            ],
            vec![json!({"id": "b-6"})],
            vec![
                json!({"id": "b-1", "org_id": "org-1"}),
                json!({"id": "b-2", "org_id": "org-1"}),
                json!({"id": "b-3", "org_id": "org-1"}),
                json!({"id": "b-4", "org_id": "org-1"}),
                json!({"id": "b-5", "org_id": "org-1"}),
                json!({"id": "b-6", "org_id": "org-1"}),
            ],
            vec![json!({"id": "org-1"})],
        ]);
        let answer =
            create_branch(&graph, &scope(), "org-1", &json!({"name": "Mutare"})).expect("created");
        assert_eq!(answer["size_band"], json!("large"));
        assert_eq!(props(&graph, 3)["size_band"], json!("large"));
    }

    #[test]
    fn a_revenue_of_nothing_is_refused_rather_than_recorded() {
        let graph = FakeGraph::queue(vec![vec![
            json!({"id": "org-1", "owner_id": "user-1", "org_type": "nonprofit", "size_band": "small", "currency": "USD"}),
        ]]);
        let refused = record_revenue(&graph, &scope(), "org-1", &json!({"amount": 0}));
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("must be positive"));
    }
}
