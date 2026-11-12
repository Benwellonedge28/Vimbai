#![forbid(unsafe_code)]
//! authorized-share-capital-service's records, on the store.
//!
//! A share class is a ceiling: so many shares may be authorized, some of them are issued, and the
//! difference is what is still available. Issuing and buying back move the issued count, and the
//! summary is the ledger of how far each class has been used.
//!
//! The counts are whole shares, so they are integers here rather than amounts. Money appears only in
//! what a share was sold or bought for, and that is exact.
//!
//! Kept as shipped:
//!
//! * Creating a class accepts `ordinary`, `preferential` or `none` voting rights and refuses anything
//!   else, in the service's own words.
//! * Issuing more than the class authorizes is refused ("Issuance exceeds authorized shares") and
//!   buying back more than has been issued is refused ("Buyback exceeds issued shares"). Both are
//!   checks against the class as it stands, so a buyback can free capacity for a later issue.
//! * Buying back does not reduce the authorized count, only the issued one; the difference is what
//!   the summary reports as available.
//! * The summary totals every class, issuance and buyback the caller owns, whether or not a class
//!   belongs to the same company, because a class carries no company.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, SHARE_BUYBACK, SHARE_CLASS, SHARE_ISSUANCE,
};

/// The voting rights a class may declare.
pub const VOTING_RIGHTS: [&str; 3] = ["ordinary", "preferential", "none"];

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn whole(payload: &Value, key: &str) -> Result<i64, StoreError> {
    match payload.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| StoreError::Invalid(format!("{key} must be a whole number"))),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map_err(|_| StoreError::Invalid(format!("{key} must be a whole number"))),
        _ => Err(StoreError::Invalid(format!("{key} is required"))),
    }
}

/// `/share-classes`: open a class of shares with a ceiling.
pub fn create_class<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let voting = {
        let value = text(payload, "voting_rights");
        if value.is_empty() {
            "ordinary".to_string()
        } else {
            value
        }
    };
    if !VOTING_RIGHTS.contains(&voting.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Invalid voting rights. Must be one of {VOTING_RIGHTS:?}"
        )));
    }

    let name = text(payload, "name");
    if name.trim().is_empty() {
        return Err(StoreError::Invalid("name is required".into()));
    }

    Records::new(graph, SHARE_CLASS).create(
        scope,
        &json!({
            "name": name,
            "authorized_shares": whole(payload, "authorized_shares")?,
            "issued_shares": 0,
            "par_value": wire(&amount_or_zero(payload, "par_value")?),
            "voting_rights": voting,
            "dividend_rate": wire(&amount_or_zero(payload, "dividend_rate")?),
            "rights": text(payload, "rights"),
        }),
    )
}

/// `/share-classes`: every class the caller owns.
pub fn list_classes<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, SHARE_CLASS).list(scope, 1000)?,
    ))
}

pub fn get_class<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, SHARE_CLASS).get(scope, class_id)
}

/// `/share-classes/{id}/issue`: sell shares out of the class's ceiling.
pub fn issue<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let class = get_class(graph, scope, class_id)?;
    let authorized = class
        .get("authorized_shares")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let issued = class
        .get("issued_shares")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let shares = whole(payload, "number_of_shares")?;

    if issued + shares > authorized {
        return Err(StoreError::Invalid(
            "Issuance exceeds authorized shares".into(),
        ));
    }

    let price = amount_or_zero(payload, "issue_price")?;
    let proceeds = price.times(&Money::from_i64(shares));

    let fields = json!({
        "share_class_id": class_id,
        "number_of_shares": shares,
        "issue_price": wire(&price),
        "total_proceeds": wire(&proceeds),
        "issued_to": text(payload, "issued_to"),
        "notes": text(payload, "notes"),
    });
    let saved = Records::new(graph, SHARE_ISSUANCE).create(scope, &fields)?;

    Records::new(graph, SHARE_CLASS).update(
        scope,
        class_id,
        &json!({"issued_shares": issued + shares}),
    )?;

    let mut answer = fields.clone();
    if let Some(id) = saved.get("id") {
        answer["id"] = id.clone();
    }
    Ok(answer)
}

/// `/share-classes/{id}/issuances`: everything sold out of the class.
pub fn list_issuances<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
) -> Result<Value, StoreError> {
    get_class(graph, scope, class_id)?;
    let rows = Records::new(graph, SHARE_ISSUANCE).list(scope, 1000)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| row.get("share_class_id").and_then(Value::as_str) == Some(class_id))
            .collect(),
    ))
}

/// `/share-classes/{id}/buyback`: take shares back, freeing the capacity for a later issue.
pub fn buyback<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let class = get_class(graph, scope, class_id)?;
    let issued = class
        .get("issued_shares")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let shares = whole(payload, "number_of_shares")?;

    if shares > issued {
        return Err(StoreError::Invalid("Buyback exceeds issued shares".into()));
    }

    let price = amount_or_zero(payload, "buyback_price")?;
    let cost = price.times(&Money::from_i64(shares));

    let fields = json!({
        "share_class_id": class_id,
        "number_of_shares": shares,
        "buyback_price": wire(&price),
        "total_cost": wire(&cost),
        "notes": text(payload, "notes"),
    });
    let saved = Records::new(graph, SHARE_BUYBACK).create(scope, &fields)?;

    Records::new(graph, SHARE_CLASS).update(
        scope,
        class_id,
        &json!({"issued_shares": issued - shares}),
    )?;

    let mut answer = fields.clone();
    if let Some(id) = saved.get("id") {
        answer["id"] = id.clone();
    }
    Ok(answer)
}

/// `/summary`: how far each class has been used, and what it has raised and cost.
pub fn summary<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let classes = Records::new(graph, SHARE_CLASS).list(scope, 1000)?;
    let issuances = Records::new(graph, SHARE_ISSUANCE).list(scope, 1000)?;
    let buybacks = Records::new(graph, SHARE_BUYBACK).list(scope, 1000)?;

    let mut authorized = 0_i64;
    let mut issued = 0_i64;
    let mut proceeds = Money::zero();
    let mut cost = Money::zero();
    let mut by_class = Vec::new();

    for class in &classes {
        let ceiling = class
            .get("authorized_shares")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let taken = class
            .get("issued_shares")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        authorized += ceiling;
        issued += taken;
        by_class.push(json!({
            "name": class.get("name").cloned().unwrap_or(Value::Null),
            "authorized": ceiling,
            "issued": taken,
            "available": ceiling - taken,
        }));
    }
    for row in &issuances {
        proceeds = proceeds.plus(&stored_amount(row, "total_proceeds")?);
    }
    for row in &buybacks {
        cost = cost.plus(&stored_amount(row, "total_cost")?);
    }

    Ok(json!({
        "total_classes": classes.len(),
        "total_authorized": authorized,
        "total_issued": issued,
        "total_proceeds": wire(&proceeds),
        "total_buyback_cost": wire(&cost),
        "by_class": by_class,
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
    fn a_new_class_has_issued_nothing() {
        let graph = FakeGraph::returning(vec![json!({"id": "c-1"})]);
        create_class(
            &graph,
            &scope(),
            &json!({"name": "Ordinary", "authorized_shares": 1000, "par_value": "0.10"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["issued_shares"], json!(0));
        assert_eq!(params["props"]["voting_rights"], json!("ordinary"));
        assert_eq!(params["props"]["par_value"], json!("0.100000"));
    }

    #[test]
    fn voting_rights_outside_the_three_are_refused() {
        let graph = FakeGraph::returning(vec![json!({"id": "c-1"})]);
        let error = create_class(
            &graph,
            &scope(),
            &json!({"name": "Ordinary", "authorized_shares": 10, "voting_rights": "weighted"}),
        )
        .expect_err("refused");
        assert!(
            error.to_string().contains("Invalid voting rights"),
            "{error}"
        );
    }

    #[test]
    fn issuing_beyond_the_ceiling_is_refused() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "c-1", "authorized_shares": 1000, "issued_shares": 900
        })]);
        let error = issue(
            &graph,
            &scope(),
            "c-1",
            &json!({"number_of_shares": 101, "issue_price": "2"}),
        )
        .expect_err("refused");
        assert_eq!(error.to_string(), "Issuance exceeds authorized shares");
    }

    #[test]
    fn an_issue_proceeds_exactly_and_moves_the_issued_count() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "c-1", "authorized_shares": 1000000, "issued_shares": 0}),
        ]);
        let answer = issue(
            &graph,
            &scope(),
            "c-1",
            &json!({"number_of_shares": 3, "issue_price": "123456789012345678901.55"}),
        )
        .expect("issued");
        // 3 x the price, with no digit lost
        assert_eq!(
            answer["total_proceeds"],
            json!("370370367037037036704.650000")
        );
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["issued_shares"], json!(3));
    }

    #[test]
    fn buying_back_more_than_was_issued_is_refused() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "c-1", "authorized_shares": 1000, "issued_shares": 10
        })]);
        let error = buyback(
            &graph,
            &scope(),
            "c-1",
            &json!({"number_of_shares": 11, "buyback_price": "1"}),
        )
        .expect_err("refused");
        assert_eq!(error.to_string(), "Buyback exceeds issued shares");
    }

    #[test]
    fn a_buyback_frees_room_without_lowering_the_ceiling() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "c-1", "authorized_shares": 1000, "issued_shares": 400
        })]);
        buyback(
            &graph,
            &scope(),
            "c-1",
            &json!({"number_of_shares": 150, "buyback_price": "2.50"}),
        )
        .expect("bought back");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["issued_shares"], json!(250));
        assert!(
            params["patch"].get("authorized_shares").is_none(),
            "the ceiling changed: {params:?}"
        );
    }

    #[test]
    fn the_summary_reports_what_is_left_in_each_class() {
        let graph = FakeGraph::queue(vec![
            // classes
            vec![
                json!({"id": "c-1", "name": "Ordinary", "authorized_shares": 1000, "issued_shares": 400}),
                json!({"id": "c-2", "name": "Preference", "authorized_shares": 500, "issued_shares": 500}),
            ],
            // issuances
            vec![json!({"id": "i-1", "total_proceeds": "1000.50"})],
            // buybacks
            vec![json!({"id": "b-1", "total_cost": "250.25"})],
        ]);
        let answer = summary(&graph, &scope()).expect("summarised");
        assert_eq!(answer["total_classes"], json!(2));
        assert_eq!(answer["total_authorized"], json!(1500));
        assert_eq!(answer["total_issued"], json!(900));
        assert_eq!(answer["total_proceeds"], json!("1000.500000"));
        assert_eq!(answer["total_buyback_cost"], json!("250.250000"));
        assert_eq!(answer["by_class"][0]["available"], json!(600));
        assert_eq!(answer["by_class"][1]["available"], json!(0));
    }
}
