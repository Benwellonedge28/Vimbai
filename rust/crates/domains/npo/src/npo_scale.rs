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
    BookScope, Graph, RecordKind, Records, StoreError, NPO_SCALE_BRANCH, NPO_SCALE_EXPENSE,
    NPO_SCALE_ORG, NPO_SCALE_PURCHASE, NPO_SCALE_REVENUE, NPO_SCALE_VENDOR,
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
    if !amount.is_zero() && amount.is_negative() {
        return Err(refused("amount must be positive"));
    }
    if amount.is_zero() {
        return Err(refused("amount must be positive"));
    }

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
            "currency": or_default(payload, "currency", field(&org, "currency")),
            "received_at": text(payload, "received_at"),
        }),
    )?;
    Ok(json!({ "service": "npo-scale-service", "revenue": revenue }))
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
