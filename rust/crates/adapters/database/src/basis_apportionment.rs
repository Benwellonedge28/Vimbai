//! The apportionment arithmetic behind basis-apportionment-service, on the store.
//!
//! One cost is spread over cost centres in proportion to a basis: floor area, volume, asset cost,
//! net book value, replacement value, head count or requisitions. The proportions are ratios of the
//! basis figures, so they are exact, and the amounts are split with `Money::allocate`, which is a
//! largest-remainder split: the parts add back to the cost being apportioned, to the last place held.
//!
//! That last property is the reason for using it. Python multiplied the total by each proportion and
//! summed the results in floats, so a cost of 5000 spread over three equal centres left 0.0000000001
//! of a cent behind. Here nothing is left behind: the parts are the total, and `unapportioned` is
//! zero whenever there is a basis to spread over. The one case that still leaves money behind is a
//! basis made entirely of zeros, where there is no proportion to spread by at all.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use crate::amounts::{amount, stored_amount, wire};
use crate::{BookScope, Graph, Records, StoreError, APPORTIONMENT_RESULT, APPORTIONMENT_RULE};

/// The places an apportioned amount is held to, which is the precision the store keeps.
const PLACES: i64 = 6;

fn text<'a>(payload: &'a Value, key: &str) -> Result<&'a str, StoreError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid(format!("{key} is required")))
}

fn stored_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn rule_json(row: &Value) -> Value {
    json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "user_id": row.get("user_id").cloned().unwrap_or(Value::Null),
        "book_id": row.get("book_id").cloned().unwrap_or(Value::Null),
        "cost_type": stored_text(row, "cost_type"),
        "basis": stored_text(row, "basis"),
        "description": stored_text(row, "description"),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    })
}

fn result_json(row: &Value) -> Result<Value, StoreError> {
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "user_id": row.get("user_id").cloned().unwrap_or(Value::Null),
        "book_id": row.get("book_id").cloned().unwrap_or(Value::Null),
        "cost_type": stored_text(row, "cost_type"),
        "total_cost": wire(&stored_amount(row, "total_cost")?),
        "basis": stored_text(row, "basis"),
        "cost_centres": row.get("cost_centres").cloned().unwrap_or(json!([])),
        "total_apportioned": wire(&stored_amount(row, "total_apportioned")?),
        "unapportioned": wire(&stored_amount(row, "unapportioned")?),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

/// `/rules/create`: the standing instruction that says which basis a cost type follows.
pub fn create_rule<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let record = json!({
        "cost_type": text(payload, "cost_type")?,
        "basis": text(payload, "basis")?,
        "description": text(payload, "description")?,
    });
    let saved = Records::new(graph, APPORTIONMENT_RULE).create(scope, &record)?;
    Ok(rule_json(&saved))
}

/// `/rules`: the caller's standing instructions.
pub fn rules<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let rows = Records::new(graph, APPORTIONMENT_RULE)
        .list(scope, 500)?
        .iter()
        .map(rule_json)
        .collect::<Vec<_>>();
    Ok(json!({ "rules": rows }))
}

/// One basis: the name the service publishes and the key the caller's figure arrives under.
struct Basis {
    name: &'static str,
    key: &'static str,
}

const BASES: [Basis; 7] = [
    Basis {
        name: "floor_area",
        key: "floor_area",
    },
    Basis {
        name: "volume",
        key: "volume",
    },
    Basis {
        name: "cost",
        key: "asset_cost",
    },
    Basis {
        name: "net_book_value",
        key: "net_book_value",
    },
    Basis {
        name: "replacement_value",
        key: "replacement_value",
    },
    Basis {
        name: "number_of_personnel",
        key: "number_of_personnel",
    },
    Basis {
        name: "number_of_requisitions",
        key: "number_of_requisitions",
    },
];

/// One cost split across cost centres by `index` into [`BASES`], stored against the Book.
pub fn apportion<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    index: usize,
    payload: &Value,
) -> Result<Value, StoreError> {
    let basis = BASES
        .get(index)
        .ok_or_else(|| StoreError::Invalid("unknown apportionment basis".into()))?;
    let cost_type = text(payload, "cost_type")?.to_owned();
    let total_cost = amount(payload, "total_cost")?;

    let centres = payload
        .get("cost_centre_data")
        .and_then(Value::as_array)
        .ok_or_else(|| StoreError::Invalid("cost_centre_data must be a list".into()))?;

    let mut names = Vec::with_capacity(centres.len());
    let mut ids = Vec::with_capacity(centres.len());
    let mut weights = Vec::with_capacity(centres.len());
    for centre in centres {
        names.push(text(centre, "cost_centre_name")?.to_owned());
        ids.push(text(centre, "cost_centre_id")?.to_owned());
        // a centre with no figure for the basis takes none of the cost, as it always did
        weights.push(amount(centre, basis.key).unwrap_or_else(|_| Money::zero()));
    }

    let basis_total = Money::sum(&weights);
    let has_basis = !basis_total.is_zero() && !basis_total.is_negative();

    // Largest remainder, so the parts are the cost rather than a rounding of it.
    let parts = if has_basis {
        total_cost.allocate(&weights, PLACES)
    } else {
        Vec::new()
    };

    let mut rows = Vec::with_capacity(parts.len());
    let mut apportioned = Money::zero();
    for position in 0..parts.len() {
        let proportion = weights[position].over(&basis_total);
        let part = parts[position].clone();
        apportioned = apportioned.plus(&part);
        rows.push(json!({
            "cost_centre_id": ids[position],
            "cost_centre_name": names[position],
            basis.key: wire(&weights[position]),
            "proportion": wire(&proportion),
            "apportioned_cost": wire(&part),
        }));
    }
    let unapportioned = total_cost.minus(&apportioned);

    let record = json!({
        "cost_type": cost_type,
        "total_cost": total_cost.to_fixed(PLACES),
        "basis": basis.name,
        "cost_centres": rows,
        "total_apportioned": apportioned.to_fixed(PLACES),
        "unapportioned": unapportioned.to_fixed(PLACES),
    });
    let saved = Records::new(graph, APPORTIONMENT_RESULT).create(scope, &record)?;
    result_json(&saved)
}

/// `/results`: what the caller has apportioned, optionally for one cost type.
pub fn results<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    cost_type: Option<&str>,
) -> Result<Value, StoreError> {
    let wanted = cost_type.unwrap_or_default();
    let mut rows = Vec::new();
    for row in Records::new(graph, APPORTIONMENT_RESULT).list(scope, 500)? {
        if !wanted.is_empty() && stored_text(&row, "cost_type") != wanted {
            continue;
        }
        rows.push(result_json(&row)?);
    }
    Ok(json!({ "results": rows }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn centres(basis: &str, figures: &[(&str, u64)]) -> Value {
        json!(figures
            .iter()
            .map(|(name, figure)| json!({
                "cost_centre_id": format!("cc-{name}"),
                "cost_centre_name": name,
                basis: figure,
            }))
            .collect::<Vec<_>>())
    }

    fn apportioned(graph: &FakeGraph) -> Value {
        graph.last().1["props"]["cost_centres"].clone()
    }

    fn props(graph: &FakeGraph) -> Value {
        graph.last().1["props"].clone()
    }

    #[test]
    fn rent_follows_the_floor_area_of_each_centre() {
        // 1000 of floor over 5000 square metres: 200/1000 for the first, 30/1000 for the last.
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "ap-1"}})]);
        apportion(
            &graph,
            &scope(),
            0,
            &json!({
                "cost_type": "rent",
                "total_cost": 1200,
                "cost_centre_data": centres("floor_area", &[("machining", 200), ("assembly", 300), ("stores", 500)]),
            }),
        )
        .unwrap();

        let rows = apportioned(&graph);
        assert_eq!(rows[0]["apportioned_cost"], json!(240.0));
        assert_eq!(rows[1]["apportioned_cost"], json!(360.0));
        assert_eq!(rows[2]["apportioned_cost"], json!(600.0));
        assert_eq!(rows[0]["proportion"], json!(0.2));
        assert_eq!(props(&graph)["basis"], json!("floor_area"));

        // the parts are the whole cost, to the last place
        let total = props(&graph)["total_apportioned"].clone();
        assert_eq!(total, json!("1200.000000"));
        assert_eq!(props(&graph)["unapportioned"], json!("0.000000"));
    }

    #[test]
    fn a_third_of_a_cent_is_not_lost_between_the_parts() {
        // 100 split three ways is 33.333333 three times, and the remainder is given to one centre so
        // the parts add back to the 100 being apportioned rather than to 99.999999.
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "ap-2"}})]);
        apportion(
            &graph,
            &scope(),
            1,
            &json!({
                "cost_type": "rates",
                "total_cost": "5000.000001",
                "cost_centre_data": centres("volume", &[("a", 1), ("b", 1), ("c", 1)]),
            }),
        )
        .unwrap();

        let rows = apportioned(&graph);
        let parts: Vec<String> = (0..3)
            .map(|index| rows[index]["apportioned_cost"].to_string())
            .collect();
        let sum: Money = parts
            .iter()
            .map(|part| Money::parse(part.trim_matches('"')).unwrap())
            .fold(Money::zero(), |acc, part| acc.plus(&part));
        assert_eq!(sum.to_fixed(6), "5000.000001");
        assert_eq!(props(&graph)["unapportioned"], json!("0.000000"));
    }

    #[test]
    fn a_basis_of_zero_leaves_the_cost_unapportioned_rather_than_inventing_a_share() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "ap-3"}})]);
        apportion(
            &graph,
            &scope(),
            4,
            &json!({
                "cost_type": "insurance",
                "total_cost": 900,
                "cost_centre_data": centres("replacement_value", &[("a", 0), ("b", 0)]),
            }),
        )
        .unwrap();

        assert_eq!(apportioned(&graph).as_array().unwrap().len(), 0);
        assert_eq!(props(&graph)["unapportioned"], json!("900.000000"));
    }

    #[test]
    fn a_centre_without_a_figure_for_the_basis_is_named_with_a_zero_share() {
        // 300 of asset cost belongs to one centre; the other states none, so it takes none. It is
        // still named in the breakdown, at zero, which is what the service always returned.
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "ap-4"}})]);
        let mut data = centres("asset_cost", &[("a", 300)])
            .as_array()
            .cloned()
            .unwrap();
        data.push(json!({"cost_centre_id": "cc-b", "cost_centre_name": "b"}));
        apportion(
            &graph,
            &scope(),
            2,
            &json!({"cost_type": "depreciation", "total_cost": 750, "cost_centre_data": data}),
        )
        .unwrap();

        let rows = apportioned(&graph);
        assert_eq!(rows.as_array().unwrap().len(), 2);
        assert_eq!(rows[0]["apportioned_cost"], json!(750.0));
        assert_eq!(rows[1]["cost_centre_name"], json!("b"));
        assert_eq!(rows[1]["apportioned_cost"], json!(0.0));
        assert_eq!(props(&graph)["unapportioned"], json!("0.000000"));
    }

    #[test]
    fn a_rule_states_which_basis_a_cost_follows_and_is_kept_in_the_callers_book() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "rule-1"}})]);
        create_rule(
            &graph,
            &scope(),
            &json!({
                "cost_type": "rent",
                "basis": "floor_area",
                "description": "Rent follows the space each centre occupies",
            }),
        )
        .unwrap();
        assert_eq!(graph.last().1["book_id"], json!("book-1"));
        assert_eq!(graph.last().1["props"]["basis"], json!("floor_area"));

        let listed = rules(&graph, &scope()).unwrap();
        assert_eq!(listed["rules"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn results_are_listed_within_the_book_and_narrowed_by_cost_type() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"id": "ap-1", "cost_type": "rent"}}),
            json!({"x": {"id": "ap-2", "cost_type": "rates"}}),
        ]);
        let all = results(&graph, &scope(), None).unwrap();
        assert_eq!(all["results"].as_array().unwrap().len(), 2);

        let rent = results(&graph, &scope(), Some("rent")).unwrap();
        assert_eq!(rent["results"].as_array().unwrap().len(), 1);
        assert_eq!(rent["results"][0]["id"], json!("ap-1"));
    }
}
