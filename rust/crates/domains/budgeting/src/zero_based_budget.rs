#![forbid(unsafe_code)]
//! zero-based-budgeting-service: budget packages built up from nothing, each line argued for on its own.
//!
//! A package holds its items, and every question about a package - what it totals, where it stands, which of its items
//! was reprioritised - is answered by rewriting the package. So the package is the record, and its items are part of it
//! rather than records of their own, which is how the service kept them.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a package starts as a draft and its total is the sum of its items as given;
//! * adding an item recalculates the total rather than adding to it, so a package whose items were edited elsewhere
//!   cannot carry a stale figure;
//! * a priority outside one to five is refused, and a priority change may carry a status with it;
//! * a status moves only among draft, submitted, reviewed, approved and rejected;
//! * an item or package the caller does not hold is a miss;
//! * the summary counts packages, totals them, and groups them by department and by status, and an empty set answers
//!   with zeros rather than nothing.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, ZERO_BASED_PACKAGE};

/// The five states a package can be in.
const STATUSES: &[&str] = &["draft", "submitted", "reviewed", "approved", "rejected"];

fn checked_status(status: &str) -> Result<&str, StoreError> {
    if STATUSES.contains(&status) {
        Ok(status)
    } else {
        Err(refused(
            "status must be one of draft, submitted, reviewed, approved or rejected",
        ))
    }
}

/// One line of a package, with the shape the service gave it.
fn item_of(payload: &Value) -> Result<Value, StoreError> {
    let amount = Money::parse(&decimal(payload, "amount", "0"))
        .map_err(|_| refused("amount must be an amount"))?;
    let priority = field(payload, "priority").parse::<i64>().unwrap_or(3);
    if !(1..=5).contains(&priority) {
        return Err(refused("Priority must be 1-5"));
    }
    Ok(json!({
        "department": text(payload, "department"),
        "cost_center": text(payload, "cost_center"),
        "category": text(payload, "category"),
        "description": text(payload, "description"),
        "amount": wire(&amount),
        "justification": text(payload, "justification"),
        "priority": priority,
        "alternative_options": text(payload, "alternative_options"),
        "impact_if_cut": text(payload, "impact_if_cut"),
        "status": if text(payload, "status").is_empty() { "pending".to_string() } else { text(payload, "status") },
    }))
}

fn total_of(items: &[Value]) -> Money {
    let mut total = Money::zero();
    for item in items {
        total = total.plus(&Money::parse(field(item, "amount")).unwrap_or_else(|_| Money::zero()));
    }
    total
}

fn items_of(package: &Value) -> Vec<Value> {
    package
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn package_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    package_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, ZERO_BASED_PACKAGE)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == package_id)
        .ok_or(StoreError::NotFound)
}

/// `POST /packages`.
pub fn create_package(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let mut items = Vec::new();
    for item in payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        items.push(item_of(&item)?);
    }
    let total = total_of(&items);

    remember(
        graph,
        scope,
        ZERO_BASED_PACKAGE,
        json!({
            "company_id": text(payload, "company_id"),
            "period": text(payload, "period"),
            "name": name,
            "department": text(payload, "department"),
            "items": items,
            "total_amount": wire(&total),
            "status": "draft",
            "reviewer": "",
            "review_notes": "",
        }),
    )
}

/// `GET /packages/{company_id}`.
pub fn list_packages(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let company = text(query, "company_id");
    let wanted_department = optional(query, "department").unwrap_or_default();
    let wanted_status = optional(query, "status_filter")
        .or_else(|| optional(query, "status"))
        .unwrap_or_default();

    Ok(Value::Array(
        Records::new(graph, ZERO_BASED_PACKAGE)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| field(row, "company_id") == company)
            .filter(|row| {
                wanted_department.is_empty() || field(row, "department") == wanted_department
            })
            .filter(|row| wanted_status.is_empty() || field(row, "status") == wanted_status)
            .collect(),
    ))
}

/// `POST /packages/{package_id}/items`: an item is added and the total recalculated.
pub fn add_item(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    package_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let package = package_of(graph, scope, package_id)?;
    let item = item_of(payload)?;

    let mut items = items_of(&package);
    items.push(item);
    let total = total_of(&items);

    Records::new(graph, ZERO_BASED_PACKAGE).update(
        scope,
        package_id,
        &json!({"items": items, "total_amount": wire(&total)}),
    )?;

    Ok(json!({
        "package_id": package_id,
        "total_amount": wire(&total),
        "items": items.len(),
    }))
}

/// `PUT /packages/{package_id}/status`.
pub fn update_status(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    package_id: &str,
    query: &Value,
    payload: &Value,
) -> Result<Value, StoreError> {
    let status = {
        let asked = match optional(query, "status") {
            Some(value) if !value.is_empty() => value,
            _ => text(payload, "status"),
        };
        // a bare status may arrive on its own or under `status`
        let asked = if asked.is_empty() {
            query
                .get("status")
                .map(|value| value.to_string().trim_matches('"').to_string())
                .unwrap_or_default()
        } else {
            asked
        };
        checked_status(&asked)?.to_string()
    };
    package_of(graph, scope, package_id)?;
    Records::new(graph, ZERO_BASED_PACKAGE).update(
        scope,
        package_id,
        &json!({
            "status": status,
            "reviewer": optional(query, "reviewer").unwrap_or_default(),
            "review_notes": optional(query, "notes").unwrap_or_default(),
        }),
    )?;

    Ok(json!({"id": package_id, "status": status}))
}

/// `PUT /items/{item_id}/priority`: the item is found inside its package and the package written back.
pub fn set_item_priority(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    item_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let priority = field(query, "priority")
        .parse::<i64>()
        .map_err(|_| refused("Priority must be 1-5"))?;
    if !(1..=5).contains(&priority) {
        return Err(refused("Priority must be 1-5"));
    }
    let wanted_status = optional(query, "status").unwrap_or_default();

    let packages = Records::new(graph, ZERO_BASED_PACKAGE);
    for package in packages.list(scope, 2000)? {
        let mut items = items_of(&package);
        let mut found = None;
        for item in items.iter_mut() {
            if field(item, "id") == item_id {
                if let Some(object) = item.as_object_mut() {
                    object.insert("priority".to_string(), json!(priority));
                    if !wanted_status.is_empty() {
                        object.insert("status".to_string(), json!(wanted_status));
                    }
                }
                found = Some(item.clone());
                break;
            }
        }
        if let Some(item) = found {
            let total = total_of(&items);
            packages.update(
                scope,
                field(&package, "id"),
                &json!({"items": items, "total_amount": wire(&total)}),
            )?;
            return Ok(json!({
                "item_id": item_id,
                "priority": field(&item, "priority").parse::<i64>().unwrap_or(priority),
                "status": field(&item, "status"),
            }));
        }
    }

    Err(StoreError::NotFound)
}

/// `GET /summary/{company_id}`.
pub fn summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let packages: Vec<Value> = Records::new(graph, ZERO_BASED_PACKAGE)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();

    if packages.is_empty() {
        return Ok(json!({
            "company_id": company_id,
            "total_packages": 0,
            "total_budget": "0.000000",
            "by_department": {},
            "by_status": {},
        }));
    }

    let mut total = Money::zero();
    let mut by_department: Vec<(String, Money)> = Vec::new();
    let mut by_status: Vec<(String, i64)> = Vec::new();

    for package in &packages {
        let amount = Money::parse(field(package, "total_amount")).unwrap_or_else(|_| Money::zero());
        total = total.plus(&amount);

        let department = field(package, "department").to_string();
        match by_department
            .iter_mut()
            .find(|(name, _)| *name == department)
        {
            Some((_, running)) => *running = running.plus(&amount),
            None => by_department.push((department, amount)),
        }

        let status = field(package, "status").to_string();
        match by_status.iter_mut().find(|(name, _)| *name == status) {
            Some((_, count)) => *count += 1,
            None => by_status.push((status, 1)),
        }
    }

    let departments: serde_json::Map<String, Value> = by_department
        .into_iter()
        .map(|(name, amount)| (name, json!(wire(&amount))))
        .collect();
    let statuses: serde_json::Map<String, Value> = by_status
        .into_iter()
        .map(|(name, count)| (name, json!(count)))
        .collect();

    Ok(json!({
        "company_id": company_id,
        "total_packages": packages.len(),
        "total_budget": wire(&total),
        "by_department": departments,
        "by_status": statuses,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_package_starts_as_a_draft_totalled_from_its_items() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "z-1"})]]);
        let answer = create_package(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "period": "2026-Q1", "name": "Q1 build",
            "department": "Ops",
            "items": [
                {"department": "Ops", "category": "software", "description": "Licence",
                 "amount": "1200", "justification": "the team needs it"},
                {"department": "Ops", "category": "training", "description": "Course",
                 "amount": "800", "justification": "certification"}
            ]}),
        )
        .expect("created");
        assert_eq!(answer["total_amount"], json!("2000.000000"));
        assert_eq!(answer["status"], json!("draft"));
        assert_eq!(answer["items"].as_array().unwrap().len(), 2);
        // an item left without a priority sits in the middle
        assert_eq!(answer["items"][0]["priority"], json!(3));
    }

    #[test]
    fn a_package_needs_a_name_and_items_with_a_priority_between_one_and_five() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_package(&graph, &scope(), &json!({"company_id": "c-1"})).is_err());
        assert!(create_package(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "name": "Q1",
                    "items": [{"description": "Licence", "amount": "10", "priority": 9}]}),
        )
        .is_err());
    }

    #[test]
    fn adding_an_item_recalculates_the_total_rather_than_adding_to_it() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "z-1", "items": [{"amount": "1200.000000"}],
                        "total_amount": "9999.000000"})],
            vec![json!({"id": "z-1"})],
        ]);
        let answer = add_item(
            &graph,
            &scope(),
            "z-1",
            &json!({"description": "Course", "amount": "800", "justification": "certification"}),
        )
        .expect("added");
        // the stale 9999 is replaced by the real sum
        assert_eq!(answer["total_amount"], json!("2000.000000"));
        assert_eq!(answer["items"], json!(2));
    }

    #[test]
    fn an_item_added_to_a_package_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::queue(vec![vec![]]);
        assert!(matches!(
            add_item(
                &graph,
                &scope(),
                "other",
                &json!({"description": "x", "amount": "1"})
            ),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_status_outside_the_five_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        assert!(update_status(
            &graph,
            &scope(),
            "z-1",
            &json!({"status": "shredded"}),
            &json!({}),
        )
        .is_err());
    }

    #[test]
    fn an_approval_records_who_reviewed_it_and_what_they_said() {
        let graph = FakeGraph::returning(vec![json!({"id": "z-1", "status": "approved"})]);
        let answer = update_status(
            &graph,
            &scope(),
            "z-1",
            &json!({"status": "approved", "reviewer": "Rudo", "notes": "trimmed travel"}),
            &json!({}),
        )
        .expect("approved");
        assert_eq!(answer["status"], json!("approved"));
    }

    #[test]
    fn reprioritising_an_item_writes_the_package_back_and_moves_its_total_with_it() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "z-1", "items": [
                {"id": "i-1", "amount": "1200.000000", "priority": 3, "status": "pending"},
                {"id": "i-2", "amount": "800.000000", "priority": 3, "status": "pending"}
            ]})],
            vec![json!({"id": "z-1"})],
        ]);
        let answer = set_item_priority(
            &graph,
            &scope(),
            "i-1",
            &json!({"priority": "1", "status": "approved"}),
        )
        .expect("reprioritised");
        assert_eq!(answer["priority"], json!(1));
        assert_eq!(answer["status"], json!("approved"));
    }

    #[test]
    fn a_priority_outside_the_range_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        assert!(set_item_priority(&graph, &scope(), "i-1", &json!({"priority": "6"})).is_err());
    }

    #[test]
    fn an_item_nowhere_in_the_callers_packages_is_a_miss() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "z-1", "items": [{"id": "i-9"}]})]]);
        assert!(matches!(
            set_item_priority(&graph, &scope(), "i-1", &json!({"priority": "2"})),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn the_summary_totals_packages_and_groups_them_two_ways() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "z-1", "company_id": "c-1", "department": "Ops",
                   "total_amount": "2000.000000", "status": "approved"}),
            json!({"id": "z-2", "company_id": "c-1", "department": "Ops",
                   "total_amount": "500.000000", "status": "draft"}),
            json!({"id": "z-3", "company_id": "c-1", "department": "Sales",
                   "total_amount": "1500.000000", "status": "draft"}),
        ]);
        let answer = summary(&graph, &scope(), "c-1").expect("summarised");
        assert_eq!(answer["total_packages"], json!(3));
        assert_eq!(answer["total_budget"], json!("4000.000000"));
        assert_eq!(answer["by_department"]["Ops"], json!("2500.000000"));
        assert_eq!(answer["by_status"]["draft"], json!(2));
    }

    #[test]
    fn a_company_with_no_packages_answers_with_zeros() {
        let graph = FakeGraph::returning(vec![]);
        let answer = summary(&graph, &scope(), "c-9").expect("summarised");
        assert_eq!(answer["total_packages"], json!(0));
        assert_eq!(answer["total_budget"], json!("0.000000"));
        assert_eq!(answer["by_department"], json!({}));
    }

    #[test]
    fn the_listing_narrows_by_department_and_status() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "z-1", "company_id": "c-1", "department": "Ops", "status": "draft"}),
            json!({"id": "z-2", "company_id": "c-1", "department": "Sales", "status": "draft"}),
            json!({"id": "z-3", "company_id": "c-1", "department": "Ops", "status": "approved"}),
        ]);
        let ops = list_packages(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "department": "Ops"}),
        )
        .expect("listed");
        assert_eq!(ops.as_array().unwrap().len(), 2);
        let approved = list_packages(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "status_filter": "approved"}),
        )
        .expect("listed");
        assert_eq!(approved.as_array().unwrap().len(), 1);
    }
}
