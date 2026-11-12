//! The overhead absorption arithmetic behind overhead-absorption-rate-service, on the store.
//!
//! An absorption rate is one division: budgeted overhead over the budgeted base. The base decides
//! the unit - machine hours, labour hours, a cost, or a count of units - and the rate is only ever
//! meaningful to the precision of the money it divides. Carrying it in `Money` keeps a rate that
//! should be 8.125 exactly 8.125, where a float division can leave a tail that the absorbed overhead
//! then inherits.
//!
//! A base of zero absorbs nothing, so the rate is absent: the service answered 0 and stated the rate
//! as a percentage point count, and those are carried as they were, because a percentage rate is what
//! the figure means.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use crate::amounts::{amount, stored_amount, wire};
use crate::{BookScope, Graph, Records, StoreError, ABSORPTION_RATE, OAR_ANALYSIS};

fn positive(money: &Money) -> bool {
    !money.is_zero() && !money.is_negative()
}

fn text<'a>(payload: &'a Value, key: &str) -> Result<&'a str, StoreError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid(format!("{key} is required")))
}

fn text_or<'a>(payload: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    payload.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

fn stored_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn rate_json(row: &Value) -> Result<Value, StoreError> {
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "user_id": row.get("user_id").cloned().unwrap_or(Value::Null),
        "book_id": row.get("book_id").cloned().unwrap_or(Value::Null),
        "cost_centre_name": stored_text(row, "cost_centre_name"),
        "cost_centre_id": stored_text(row, "cost_centre_id"),
        "overhead_type": stored_text(row, "overhead_type"),
        "absorption_base": stored_text(row, "absorption_base"),
        "budgeted_overhead": wire(&stored_amount(row, "budgeted_overhead")?),
        "budgeted_base_units": wire(&stored_amount(row, "budgeted_base_units")?),
        "absorption_rate": wire(&stored_amount(row, "absorption_rate")?),
        "rate_per_unit": wire(&stored_amount(row, "rate_per_unit")?),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

fn analysis_json(row: &Value) -> Result<Value, StoreError> {
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "user_id": row.get("user_id").cloned().unwrap_or(Value::Null),
        "book_id": row.get("book_id").cloned().unwrap_or(Value::Null),
        "entity_name": stored_text(row, "entity_name"),
        "cost_centre_name": stored_text(row, "cost_centre_name"),
        "rate_type": stored_text(row, "rate_type"),
        "absorption_base": stored_text(row, "absorption_base"),
        "calculated_rate": wire(&stored_amount(row, "calculated_rate")?),
        "base_units_used": wire(&stored_amount(row, "base_units_used")?),
        "overhead_absorbed": wire(&stored_amount(row, "overhead_absorbed")?),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

/// The rate itself: overhead over base, or nothing when there is no base to absorb against.
fn rate(overhead: &Money, base: &Money) -> Money {
    if positive(base) {
        overhead.over(base)
    } else {
        Money::zero()
    }
}

/// The percentage form of the same rate, which is what a cost-based base is expressed in.
fn percentage(overhead: &Money, base: &Money) -> Money {
    if positive(base) {
        overhead.over(base).times(&Money::from_i64(100))
    } else {
        Money::zero()
    }
}

/// One recorded rate calculation: the names it was made under and the three figures it produced.
struct Analysis<'a> {
    entity_name: &'a str,
    cost_centre_name: &'a str,
    rate_type: &'a str,
    absorption_base: &'a str,
    calculated_rate: &'a Money,
    base_units_used: &'a Money,
    overhead_absorbed: &'a Money,
}

fn store_analysis<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    analysis: &Analysis<'_>,
) -> Result<Value, StoreError> {
    let record = json!({
        "entity_name": analysis.entity_name,
        "cost_centre_name": analysis.cost_centre_name,
        "rate_type": analysis.rate_type,
        "absorption_base": analysis.absorption_base,
        "calculated_rate": analysis.calculated_rate.to_fixed(6),
        "base_units_used": analysis.base_units_used.to_fixed(6),
        "overhead_absorbed": analysis.overhead_absorbed.to_fixed(6),
    });
    let saved = Records::new(graph, OAR_ANALYSIS).create(scope, &record)?;
    Ok(json!({"analysis_id": saved.get("id").cloned().unwrap_or(Value::Null)}))
}

/// `/calculate`: the rate for one cost centre, stored.
pub fn create_rate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "cost_centre_name")?.to_owned();
    let cost_centre_id = text(payload, "cost_centre_id")?.to_owned();
    let overhead = amount(payload, "budgeted_overhead")?;
    let base = amount(payload, "budgeted_base_units")?;
    let overhead_type = text_or(payload, "overhead_type", "production").to_owned();
    let absorption_base = text_or(payload, "absorption_base", "machine_hours").to_owned();

    let rate = rate(&overhead, &base);
    let record = json!({
        "cost_centre_name": name,
        "cost_centre_id": cost_centre_id,
        "overhead_type": overhead_type,
        "absorption_base": absorption_base,
        "budgeted_overhead": overhead.to_fixed(6),
        "budgeted_base_units": base.to_fixed(6),
        "absorption_rate": rate.to_fixed(6),
        "rate_per_unit": rate.to_fixed(6),
    });
    let saved = Records::new(graph, ABSORPTION_RATE).create(scope, &record)?;
    rate_json(&saved)
}

/// The six bases that share one shape: a rate, an analysis recorded, and an answer describing it.
///
/// The base names the key the caller's figure arrives under and the key the rate leaves under, so
/// each endpoint keeps the words it always published.
struct BaseRoute {
    key: &'static str,
    answer_key: &'static str,
    absorption_base: &'static str,
    percentage: bool,
}

const BASES: [BaseRoute; 6] = [
    BaseRoute {
        key: "budgeted_machine_hours",
        answer_key: "oar_per_machine_hour",
        absorption_base: "machine_hours",
        percentage: false,
    },
    BaseRoute {
        key: "budgeted_direct_labour_hours",
        answer_key: "oar_per_direct_labour_hour",
        absorption_base: "direct_labour_hours",
        percentage: false,
    },
    BaseRoute {
        key: "budgeted_direct_labour_cost",
        answer_key: "oar_percentage",
        absorption_base: "direct_labour_cost",
        percentage: true,
    },
    BaseRoute {
        key: "budgeted_material_cost",
        answer_key: "oar_percentage",
        absorption_base: "material_cost",
        percentage: true,
    },
    BaseRoute {
        key: "budgeted_prime_cost",
        answer_key: "oar_percentage",
        absorption_base: "prime_cost",
        percentage: true,
    },
    BaseRoute {
        key: "budgeted_units",
        answer_key: "oar_per_unit",
        absorption_base: "units",
        percentage: false,
    },
];

/// One of the six base endpoints: `index` picks the base from [`BASES`].
pub fn base_rate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    index: usize,
    payload: &Value,
) -> Result<Value, StoreError> {
    let base_route = BASES
        .get(index)
        .ok_or_else(|| StoreError::Invalid("unknown absorption base".into()))?;
    let cost_centre_name = text(payload, "cost_centre_name")?.to_owned();
    let overhead = amount(payload, "budgeted_overhead")?;
    let base = amount(payload, base_route.key)?;

    let calculated = if base_route.percentage {
        percentage(&overhead, &base)
    } else {
        rate(&overhead, &base)
    };

    let analysis = store_analysis(
        graph,
        scope,
        &Analysis {
            entity_name: &cost_centre_name,
            cost_centre_name: &cost_centre_name,
            rate_type: "departmental",
            absorption_base: base_route.absorption_base,
            calculated_rate: &calculated,
            base_units_used: &base,
            overhead_absorbed: &overhead,
        },
    )?;

    let mut answer = json!({
        "cost_centre_name": cost_centre_name,
        "absorption_base": base_route.absorption_base,
        "budgeted_overhead": wire(&overhead),
        base_route.key: wire(&base),
        base_route.answer_key: wire(&calculated),
    });
    answer["analysis_id"] = analysis["analysis_id"].clone();
    Ok(answer)
}

/// `/blanket-rate`: one rate for the whole factory, from totals rather than a cost centre.
pub fn blanket_rate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entity_name = text(payload, "entity_name")?.to_owned();
    let absorption_base = text(payload, "absorption_base")?.to_owned();
    let overhead = amount(payload, "total_budgeted_overhead")?;
    let base = amount(payload, "total_base_units")?;

    let blanket = rate(&overhead, &base);
    let analysis = store_analysis(
        graph,
        scope,
        &Analysis {
            entity_name: &entity_name,
            cost_centre_name: "entire_factory",
            rate_type: "blanket",
            absorption_base: &absorption_base,
            calculated_rate: &blanket,
            base_units_used: &base,
            overhead_absorbed: &overhead,
        },
    )?;

    Ok(json!({
        "entity_name": entity_name,
        "rate_type": "blanket",
        "absorption_base": absorption_base,
        "total_budgeted_overhead": wire(&overhead),
        "total_base_units": wire(&base),
        "blanket_oar": wire(&blanket),
        "analysis_id": analysis["analysis_id"].clone(),
    }))
}

/// `/absorbed-overhead`: what a rate recovers from the activity that actually happened.
pub fn absorbed_overhead(payload: &Value) -> Result<Value, StoreError> {
    let cost_centre_name = text(payload, "cost_centre_name")?.to_owned();
    let units = amount(payload, "actual_base_units")?;
    let rate = amount(payload, "absorption_rate")?;

    let absorbed = units.times(&rate);
    Ok(json!({
        "cost_centre_name": cost_centre_name,
        "actual_base_units": wire(&units),
        "absorption_rate": wire(&rate),
        "overhead_absorbed": wire(&absorbed),
    }))
}

/// `/rates`: the stored rates, optionally narrowed to one cost centre.
pub fn rates<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    cost_centre_id: Option<&str>,
) -> Result<Value, StoreError> {
    let wanted = cost_centre_id.unwrap_or_default();
    let mut rows = Vec::new();
    for row in Records::new(graph, ABSORPTION_RATE).list(scope, 500)? {
        if !wanted.is_empty() && stored_text(&row, "cost_centre_id") != wanted {
            continue;
        }
        rows.push(rate_json(&row)?);
    }
    Ok(json!({ "rates": rows }))
}

/// `/analyses`: every rate the caller has had recorded.
pub fn analyses<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let mut rows = Vec::new();
    for row in Records::new(graph, OAR_ANALYSIS).list(scope, 500)? {
        rows.push(analysis_json(&row)?);
    }
    Ok(json!({ "analyses": rows }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn props(graph: &FakeGraph) -> Value {
        graph.last().1["props"].clone()
    }

    #[test]
    fn a_rate_is_the_overhead_of_a_century_split_three_ways() {
        // 100 split three ways is 33.333333 a share, and the rate keeps every one of those digits.
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "r-1"}})]);
        create_rate(
            &graph,
            &scope(),
            &json!({
                "cost_centre_name": "Finishing",
                "cost_centre_id": "cc-1",
                "budgeted_overhead": 100,
                "budgeted_base_units": 3,
            }),
        )
        .unwrap();

        let props = props(&graph);
        assert_eq!(props["absorption_rate"], json!("33.333333"));
        // an unstated type and base keep the service defaults
        assert_eq!(props["overhead_type"], json!("production"));
        assert_eq!(props["absorption_base"], json!("machine_hours"));
    }

    #[test]
    fn a_base_of_zero_absorbs_nothing_rather_than_dividing_by_it() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "r-2"}})]);
        create_rate(
            &graph,
            &scope(),
            &json!({
                "cost_centre_name": "Idle",
                "cost_centre_id": "cc-2",
                "budgeted_overhead": 8000,
                "budgeted_base_units": 0,
            }),
        )
        .unwrap();

        let props = props(&graph);
        assert_eq!(props["absorption_rate"], json!("0.000000"));
        assert_eq!(props["budgeted_overhead"], json!("8000.000000"));
    }

    #[test]
    fn each_base_answers_under_its_own_name_and_records_the_calculation() {
        // machine hours: 12500 over 2000 hours is 6.25 an hour
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "an-1"}})]);
        let answer = base_rate(
            &graph,
            &scope(),
            0,
            &json!({
                "cost_centre_name": "Machining",
                "budgeted_overhead": 12500,
                "budgeted_machine_hours": 2000,
            }),
        )
        .unwrap();
        assert_eq!(answer["oar_per_machine_hour"], json!(6.25));
        assert_eq!(answer["absorption_base"], json!("machine_hours"));
        assert_eq!(props(&graph)["rate_type"], json!("departmental"));

        // direct labour cost: 12500 over 50000 of labour is 25 percent
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "an-2"}})]);
        let answer = base_rate(
            &graph,
            &scope(),
            2,
            &json!({
                "cost_centre_name": "Assembly",
                "budgeted_overhead": 12500,
                "budgeted_direct_labour_cost": 50000,
            }),
        )
        .unwrap();
        assert_eq!(answer["oar_percentage"], json!(25.0));
        assert_eq!(
            props(&graph)["absorption_base"],
            json!("direct_labour_cost")
        );

        // units: 12500 over 2500 units is 5 a unit
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "an-3"}})]);
        let answer = base_rate(
            &graph,
            &scope(),
            5,
            &json!({
                "cost_centre_name": "Packing",
                "budgeted_overhead": 12500,
                "budgeted_units": 2500,
            }),
        )
        .unwrap();
        assert_eq!(answer["oar_per_unit"], json!(5.0));
    }

    #[test]
    fn a_base_cost_the_caller_did_not_supply_is_refused_not_treated_as_zero() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "an-1"}})]);
        let refused = base_rate(
            &graph,
            &scope(),
            3,
            &json!({"cost_centre_name": "Stores", "budgeted_overhead": 100}),
        );
        assert!(matches!(refused, Err(StoreError::Invalid(_))));
        assert_eq!(graph.statements(), 0);
    }

    #[test]
    fn a_blanket_rate_is_one_rate_for_the_whole_factory() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "an-4"}})]);
        let answer = blanket_rate(
            &graph,
            &scope(),
            &json!({
                "entity_name": "Works",
                "absorption_base": "direct_labour_hours",
                "total_budgeted_overhead": 900000,
                "total_base_units": 40000,
            }),
        )
        .unwrap();

        assert_eq!(answer["blanket_oar"], json!(22.5));
        assert_eq!(answer["rate_type"], json!("blanket"));
        let props = props(&graph);
        assert_eq!(props["cost_centre_name"], json!("entire_factory"));
        assert_eq!(props["overhead_absorbed"], json!("900000.000000"));
    }

    #[test]
    fn absorbed_overhead_is_the_activity_times_the_rate_and_nothing_is_stored() {
        let graph = FakeGraph::returning(vec![]);
        let answer = absorbed_overhead(&json!({
            "cost_centre_name": "Machining",
            "actual_base_units": 2100,
            "absorption_rate": "6.25",
        }))
        .unwrap();
        assert_eq!(answer["overhead_absorbed"], json!(13125.0));
        // 2100 hours at 6.25 over-absorbs the 12500 budgeted by 625
        assert_eq!(graph.statements(), 0);
    }

    #[test]
    fn rates_and_analyses_are_listed_within_the_callers_book() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"id": "r-1", "cost_centre_id": "cc-1"}}),
            json!({"x": {"id": "r-2", "cost_centre_id": "cc-2"}}),
        ]);
        let all = rates(&graph, &scope(), None).unwrap();
        assert_eq!(all["rates"].as_array().unwrap().len(), 2);

        let one = rates(&graph, &scope(), Some("cc-2")).unwrap();
        assert_eq!(one["rates"].as_array().unwrap().len(), 1);
        assert_eq!(one["rates"][0]["id"], json!("r-2"));

        let analyses = analyses(&graph, &scope()).unwrap();
        assert_eq!(analyses["analyses"].as_array().unwrap().len(), 2);
        assert_eq!(graph.last().1["book_id"], json!("book-1"));
    }
}
