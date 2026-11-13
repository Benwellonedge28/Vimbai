#![forbid(unsafe_code)]
//! benefits-admin-service: the benefit plans a Book offers, who is enrolled in them, and the leave each person has
//! accrued.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a plan's TYPE is one of five - pension, medical, dental, life insurance, leave - and anything else is refused by
//!   name rather than stored; a plan arrives active;
//! * a listing is the caller's own plans, narrowed by type only when a type is asked for;
//! * an enrollment needs a plan the caller holds - another Book's plan answers as missing - and the same person
//!   cannot be enrolled in the same plan twice; the duplicate is a refusal, not a second record;
//! * leave is a RUNNING BALANCE rather than an entry: each accrual carries forward the balance of the last accrual for
//!   that person and that kind of leave, adds the days accrued and takes off the days taken, so the balance is the
//!   history of everything recorded rather than this one movement. Days are reckoned exactly, not in floats;
//! * leave of a kind outside the four is refused by name, and a person's leave is listed newest first, by kind when
//!   one is asked for.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::read::{decimal, field, optional, or_default, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A benefit plan.
pub const PLAN: RecordKind = RecordKind::new("BenefitPlan", "OWNS_BENEFIT_PLAN");

/// A person's enrollment in a plan.
pub const ENROLLMENT: RecordKind = RecordKind::new("BenefitEnrollment", "OWNS_ENROLLMENT");

/// A person's leave accrual.
pub const ACCRUAL: RecordKind = RecordKind::new("LeaveAccrual", "OWNS_LEAVE_ACCRUAL");

/// The kinds of plan a Book may offer.
pub const PLAN_TYPES: [&str; 5] = ["pension", "medical", "dental", "life_insurance", "leave"];

/// The kinds of leave a person accrues.
pub const LEAVE_TYPES: [&str; 4] = ["annual", "sick", "maternity", "compassionate"];

/// A decimal written back as a JSON number, which is the shape the shipped answers carried.
fn as_number(value: &str) -> Value {
    match Money::parse(value) {
        Ok(amount) => serde_json::from_str(&amount.to_fixed(2)).unwrap_or_else(|_| json!(0)),
        Err(_) => json!(0),
    }
}

fn plan_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "name": field(record, "name"),
        "plan_type": field(record, "plan_type"),
        "description": field(record, "description"),
        "employer_contribution_pct": as_number(&decimal(record, "employer_contribution_pct", "0")),
        "employee_contribution_pct": as_number(&decimal(record, "employee_contribution_pct", "0")),
        "eligibility_months": decimal(record, "eligibility_months", "0"),
        "status": or_default(record, "status", "active"),
        "created_at": field(record, "created_at"),
    })
}

fn enrollment_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "employee_id": field(record, "employee_id"),
        "plan_id": field(record, "plan_id"),
        "enrollment_date": field(record, "enrollment_date"),
        "status": or_default(record, "status", "active"),
        "beneficiary": field(record, "beneficiary"),
    })
}

fn accrual_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "employee_id": field(record, "employee_id"),
        "leave_type": field(record, "leave_type"),
        "period": field(record, "period"),
        "accrued_days": as_number(&decimal(record, "accrued_days", "0")),
        "taken_days": as_number(&decimal(record, "taken_days", "0")),
        "balance_days": as_number(&decimal(record, "balance_days", "0")),
        "created_at": field(record, "created_at"),
    })
}

fn days(value: &Value, key: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(value, key, "0"))
        .map_err(|_| refused(&format!("{key} must be a number of days")))
}

/// `POST /plans`: offer a benefit plan.
pub fn offer_benefit_plan(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let plan_type = text(payload, "plan_type");
    if !PLAN_TYPES.contains(&plan_type.as_str()) {
        return Err(refused(&format!(
            "Invalid plan type. Must be one of {}",
            PLAN_TYPES.join(", ")
        )));
    }
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "name": name,
        "plan_type": plan_type,
        "description": text(payload, "description"),
        "employer_contribution_pct": decimal(payload, "employer_contribution_pct", "0"),
        "employee_contribution_pct": decimal(payload, "employee_contribution_pct", "0"),
        "eligibility_months": decimal(payload, "eligibility_months", "0"),
        "status": "active",
        "created_at": text(payload, "created_at"),
    });
    let created = Records::new(graph, PLAN).create(scope, &fields)?;
    Ok(plan_answer(&created))
}

/// `GET /plans`: the caller's plans, narrowed by type when one is asked for.
pub fn benefit_plans_offered(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "plan_type");
    let held = Records::new(graph, PLAN).list(scope, 1000)?;
    let mine: Vec<Value> = held
        .iter()
        .filter(|plan| wanted.is_empty() || field(plan, "plan_type") == wanted)
        .map(plan_answer)
        .collect();
    Ok(Value::Array(mine))
}

/// `POST /enroll`: enroll a person in one of the caller's plans.
pub fn enroll_employee(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let employee_id = text(payload, "employee_id");
    if employee_id.is_empty() {
        return Err(refused("employee_id is required"));
    }
    let plan_id = text(payload, "plan_id");
    // the plan must be one this Book holds; another Book's plan is simply not found
    Records::new(graph, PLAN).get(scope, &plan_id)?;

    let already = Records::new(graph, ENROLLMENT).list(scope, 1000)?;
    if already.iter().any(|enrollment| {
        field(enrollment, "employee_id") == employee_id && field(enrollment, "plan_id") == plan_id
    }) {
        return Err(StoreError::Conflict(
            "Employee already enrolled in this plan".to_string(),
        ));
    }
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "employee_id": employee_id,
        "plan_id": plan_id,
        "enrollment_date": text(payload, "enrollment_date"),
        "status": "active",
        "beneficiary": text(payload, "beneficiary"),
    });
    let created = Records::new(graph, ENROLLMENT).create(scope, &fields)?;
    Ok(enrollment_answer(&created))
}

/// `GET /employee/{id}/enrollments`: what a person is enrolled in.
pub fn list_employee_enrollments(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    employee_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, ENROLLMENT).list(scope, 1000)?;
    let mine: Vec<Value> = held
        .iter()
        .filter(|enrollment| field(enrollment, "employee_id") == employee_id)
        .map(enrollment_answer)
        .collect();
    Ok(Value::Array(mine))
}

/// `POST /leave/accrue`: record a leave movement, carrying the balance forward.
pub fn accrue_leave(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let employee_id = text(payload, "employee_id");
    if employee_id.is_empty() {
        return Err(refused("employee_id is required"));
    }
    let leave_type = text(payload, "leave_type");
    if !LEAVE_TYPES.contains(&leave_type.as_str()) {
        return Err(refused(&format!(
            "Invalid leave type. Must be one of {}",
            LEAVE_TYPES.join(", ")
        )));
    }
    let accrued = days(payload, "accrued_days")?;
    // the shipped endpoint would not accept a movement of nothing
    if accrued.is_zero() || accrued.is_negative() {
        return Err(refused("accrued_days must be greater than zero"));
    }
    let taken = days(payload, "taken_days")?;

    // the balance carries forward from the last accrual of this kind for this person
    let held = Records::new(graph, ACCRUAL).list(scope, 1000)?;
    let mut earlier: Vec<&Value> = held
        .iter()
        .filter(|accrual| {
            field(accrual, "employee_id") == employee_id
                && field(accrual, "leave_type") == leave_type
        })
        .collect();
    earlier.sort_by_key(|accrual| field(accrual, "created_at").to_string());
    let carried = earlier
        .last()
        .and_then(|accrual| Money::parse(field(accrual, "balance_days")).ok())
        .unwrap_or_else(Money::zero);
    let balance = carried.plus(&accrued).minus(&taken);

    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "employee_id": employee_id,
        "leave_type": leave_type,
        "period": text(payload, "period"),
        "accrued_days": decimal(payload, "accrued_days", "0"),
        "taken_days": decimal(payload, "taken_days", "0"),
        "balance_days": balance.to_fixed(2),
        "created_at": text(payload, "created_at"),
    });
    let created = Records::new(graph, ACCRUAL).create(scope, &fields)?;
    Ok(accrual_answer(&created))
}

/// `GET /employee/{id}/leave`: a person's leave, newest first, by kind when one is asked for.
pub fn list_employee_leave(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    employee_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(payload, "leave_type").unwrap_or_default();
    let held = Records::new(graph, ACCRUAL).list(scope, 1000)?;
    let mut mine: Vec<Value> = held
        .iter()
        .filter(|accrual| {
            field(accrual, "employee_id") == employee_id
                && (wanted.is_empty() || field(accrual, "leave_type") == wanted)
        })
        .map(accrual_answer)
        .collect();
    mine.sort_by(|left, right| {
        right["created_at"]
            .as_str()
            .unwrap_or("")
            .cmp(left["created_at"].as_str().unwrap_or(""))
    });
    Ok(Value::Array(mine))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_plan(graph: &MemoryGraph, plan_type: &str) -> Value {
        offer_benefit_plan(
            graph,
            &scope(),
            &json!({
                "name": "Medical Aid",
                "plan_type": plan_type,
                "employer_contribution_pct": 7.5,
                "employee_contribution_pct": 2.5,
                "eligibility_months": 3,
                "created_at": "2026-01-01T09:00:00Z",
            }),
        )
        .expect("a plan")
    }

    #[test]
    fn a_plan_of_an_unknown_kind_is_refused_by_name() {
        let graph = MemoryGraph::new();
        let made = a_plan(&graph, "medical");
        assert_eq!(made["status"], json!("active"));
        assert_eq!(made["employer_contribution_pct"], json!(7.5));
        assert!(offer_benefit_plan(
            &graph,
            &scope(),
            &json!({"name": "Gym", "plan_type": "gym_membership"})
        )
        .is_err());
    }

    #[test]
    fn a_listing_narrows_by_type_only_when_one_is_asked_for() {
        let graph = MemoryGraph::new();
        a_plan(&graph, "medical");
        a_plan(&graph, "pension");
        let all = benefit_plans_offered(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(all.as_array().map(|held| held.len()), Some(2));
        let medical = benefit_plans_offered(&graph, &scope(), &json!({"plan_type": "medical"}))
            .expect("a listing");
        assert_eq!(medical.as_array().map(|held| held.len()), Some(1));
    }

    #[test]
    fn the_same_person_cannot_be_enrolled_twice() {
        let graph = MemoryGraph::new();
        let plan = a_plan(&graph, "medical");
        let asked = json!({
            "employee_id": "emp-1",
            "plan_id": field(&plan, "id"),
            "beneficiary": "Tariro Moyo",
            "enrollment_date": "2026-02-01T09:00:00Z",
        });
        let first = enroll_employee(&graph, &scope(), &asked).expect("an enrollment");
        assert_eq!(first["status"], json!("active"));
        // the duplicate is a refusal, not a second record
        assert!(enroll_employee(&graph, &scope(), &asked).is_err());
        let listed = list_employee_enrollments(&graph, &scope(), "emp-1").expect("a listing");
        assert_eq!(listed.as_array().map(|held| held.len()), Some(1));
    }

    #[test]
    fn another_books_plan_cannot_be_enrolled_in() {
        let graph = MemoryGraph::new();
        let plan = a_plan(&graph, "medical");
        let refused_enrollment = enroll_employee(
            &graph,
            &BookScope::new("user-2", "book-2"),
            &json!({"employee_id": "emp-1", "plan_id": field(&plan, "id")}),
        );
        assert!(refused_enrollment.is_err());
    }

    #[test]
    fn leave_carries_its_balance_forward() {
        let graph = MemoryGraph::new();
        let first = accrue_leave(
            &graph,
            &scope(),
            &json!({
                "employee_id": "emp-1",
                "leave_type": "annual",
                "period": "2026-01",
                "accrued_days": 2.5,
                "taken_days": 0.5,
                "created_at": "2026-01-31T09:00:00Z",
            }),
        )
        .expect("an accrual");
        assert_eq!(first["balance_days"], json!(2.0));

        let second = accrue_leave(
            &graph,
            &scope(),
            &json!({
                "employee_id": "emp-1",
                "leave_type": "annual",
                "period": "2026-02",
                "accrued_days": 2.5,
                "taken_days": 1.25,
                "created_at": "2026-02-28T09:00:00Z",
            }),
        )
        .expect("an accrual");
        // 2.0 carried + 2.5 accrued - 1.25 taken
        assert_eq!(second["balance_days"], json!(3.25));

        assert!(accrue_leave(
            &graph,
            &scope(),
            &json!({"employee_id": "emp-1", "leave_type": "sabbatical", "period": "2026-03",
                    "accrued_days": 1, "created_at": "2026-03-01T09:00:00Z"})
        )
        .is_err());
        // a movement of nothing is refused, as the shipped endpoint's constraint did
        assert!(accrue_leave(
            &graph,
            &scope(),
            &json!({"employee_id": "emp-1", "leave_type": "annual", "period": "2026-03",
                    "accrued_days": 0, "created_at": "2026-03-01T09:00:00Z"})
        )
        .is_err());
    }

    #[test]
    fn a_persons_leave_is_listed_newest_first_and_by_kind() {
        let graph = MemoryGraph::new();
        for (period, created) in [
            ("2026-01", "2026-01-31T09:00:00Z"),
            ("2026-02", "2026-02-28T09:00:00Z"),
        ] {
            accrue_leave(
                &graph,
                &scope(),
                &json!({"employee_id": "emp-1", "leave_type": "annual", "period": period,
                        "accrued_days": 1, "created_at": created}),
            )
            .expect("an accrual");
        }
        accrue_leave(
            &graph,
            &scope(),
            &json!({"employee_id": "emp-1", "leave_type": "sick", "period": "2026-02",
                    "accrued_days": 1, "created_at": "2026-02-15T09:00:00Z"}),
        )
        .expect("an accrual");

        let all = list_employee_leave(&graph, &scope(), "emp-1", &json!({})).expect("a listing");
        assert_eq!(all.as_array().map(|held| held.len()), Some(3));
        assert_eq!(all[0]["period"], json!("2026-02"));
        let annual =
            list_employee_leave(&graph, &scope(), "emp-1", &json!({"leave_type": "annual"}))
                .expect("a listing");
        assert_eq!(annual.as_array().map(|held| held.len()), Some(2));
    }
}
