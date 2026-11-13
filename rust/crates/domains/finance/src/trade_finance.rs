#![forbid(unsafe_code)]
//! trade-finance-service: the instruments a company trades on - letters of credit, documentary collections, bank
//! guarantees, advance payments and factoring - and what each costs and risks.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the fee is a rate on the face amount by instrument type: 0.2% for a letter of credit, 0.1% for a documentary
//!   collection, 1.5% for a bank guarantee, nothing for an advance payment and 3% for factoring;
//! * the risk band is the face amount itself rather than anything derived from the counterparty: under 100,000 is low,
//!   under 500,000 medium, above that high;
//! * the documentation is the four papers every shipment needs, and a letter of credit adds the application and the
//!   proforma invoice;
//! * an instrument carries one of five types and nothing else is accepted;
//! * document presentation and settlement only move the status, and an instrument the company does not hold is a miss;
//! * the listing is by issue date then id, so the oldest instrument reads first.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::query::query_page;
use vimbai_store::read::{field, money, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, TRADE_INSTRUMENT};

/// The five instrument types, with the fee each is charged at.
const FEE_RATES: &[(&str, &str)] = &[
    ("letter_of_credit", "0.002"),
    ("documentary_collection", "0.001"),
    ("bank_guarantee", "0.015"),
    ("advance_payment", "0"),
    ("factoring", "0.03"),
];

fn fee_rate(instrument_type: &str) -> Option<Money> {
    FEE_RATES
        .iter()
        .find(|(code, _)| *code == instrument_type)
        .and_then(|(_, rate)| Money::parse(rate).ok())
}

fn checked(instrument_type: &str) -> Result<&str, StoreError> {
    if fee_rate(instrument_type).is_some() {
        Ok(instrument_type)
    } else {
        Err(refused(
            "instrument_type must be one of letter_of_credit, documentary_collection, bank_guarantee, \
             advance_payment or factoring",
        ))
    }
}

/// What the instrument costs, risks and needs on paper.
fn result(record: &Value) -> Result<Value, StoreError> {
    let instrument_type = field(record, "instrument_type").to_string();
    let amount = Money::parse(field(record, "amount")).unwrap_or_else(|_| Money::zero());
    let fee = amount.times(&fee_rate(&instrument_type).unwrap_or_else(Money::zero));

    let low = Money::parse("100000").unwrap_or_else(|_| Money::zero());
    let high = Money::parse("500000").unwrap_or_else(|_| Money::zero());
    let risk = if amount.as_decimal() < low.as_decimal() {
        "low"
    } else if amount.as_decimal() < high.as_decimal() {
        "medium"
    } else {
        "high"
    };

    let mut documents = vec![
        "Commercial invoice",
        "Bill of lading",
        "Certificate of origin",
        "Packing list",
    ];
    if instrument_type == "letter_of_credit" {
        documents.push("LC application");
        documents.push("Proforma invoice");
    }

    Ok(json!({
        "id": field(record, "id"),
        "company_id": field(record, "company_id"),
        "instrument_type": instrument_type,
        "amount": wire(&amount),
        "fee_estimate": wire(&fee.round(2)),
        "status": field(record, "status"),
        "risk_assessment": risk,
        "documentation_required": documents,
    }))
}

/// `POST /instruments`.
pub fn create_instrument(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let instrument_type = checked(&text(payload, "instrument_type"))?.to_string();
    let amount = money(payload, "amount")?;
    if amount.is_negative() {
        return Err(refused("amount cannot be negative"));
    }

    let record = remember(
        graph,
        scope,
        TRADE_INSTRUMENT,
        json!({
            "company_id": text(payload, "company_id"),
            "instrument_type": instrument_type,
            "counterparty": text(payload, "counterparty"),
            "amount": wire(&amount),
            "currency": or_currency(payload),
            "issue_date": text(payload, "issue_date"),
            "expiry_date": text(payload, "expiry_date"),
            "status": "issued",
            "issuing_bank": text(payload, "issuing_bank"),
            "confirming_bank": text(payload, "confirming_bank"),
        }),
    )?;

    result(&record)
}

fn or_currency(payload: &Value) -> String {
    let currency = text(payload, "currency");
    if currency.is_empty() {
        "USD".to_string()
    } else {
        currency
    }
}

/// `GET /instruments`: one company's instruments, oldest first.
pub fn list_instruments(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted_company = text(query, "company_id");
    let wanted_status = optional(query, "status").unwrap_or_default();
    let limit = query_page(query, 200, 2000);

    let mut rows: Vec<Value> = Records::new(graph, TRADE_INSTRUMENT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == wanted_company)
        .filter(|row| wanted_status.is_empty() || field(row, "status") == wanted_status)
        .collect();
    rows.sort_by(|one, two| {
        field(one, "issue_date")
            .cmp(field(two, "issue_date"))
            .then_with(|| field(one, "id").cmp(field(two, "id")))
    });
    rows.truncate(limit);

    Ok(Value::Array(rows))
}

/// The instrument, or a miss, so nothing answers for one the company does not hold.
fn instrument_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    instrument_id: &str,
    company_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, TRADE_INSTRUMENT)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == instrument_id && field(row, "company_id") == company_id)
        .ok_or(StoreError::NotFound)
}

/// `POST /instruments/{instrument_id}/present`.
pub fn present_documents(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    instrument_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    instrument_of(graph, scope, instrument_id, &text(query, "company_id"))?;
    Records::new(graph, TRADE_INSTRUMENT).update(
        scope,
        instrument_id,
        &json!({"status": "presented"}),
    )?;
    Ok(json!({"instrument_id": instrument_id, "status": "presented"}))
}

/// `POST /instruments/{instrument_id}/settle`.
pub fn settle_instrument(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    instrument_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let found = instrument_of(graph, scope, instrument_id, &text(query, "company_id"))?;
    Records::new(graph, TRADE_INSTRUMENT).update(
        scope,
        instrument_id,
        &json!({"status": "paid"}),
    )?;
    Ok(json!({
        "instrument_id": instrument_id,
        "status": "paid",
        "amount": field(&found, "amount"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn lc(amount: &str) -> Value {
        json!({"id": "i-1", "company_id": "c-1", "instrument_type": "letter_of_credit",
               "amount": amount, "status": "issued"})
    }

    #[test]
    fn a_letter_of_credit_is_charged_two_in_a_thousand() {
        let graph = FakeGraph::queue(vec![vec![lc("200000.000000")]]);
        let answer = create_instrument(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "instrument_type": "letter_of_credit",
                    "counterparty": "Sino Ltd", "amount": "200000"}),
        )
        .expect("created");
        assert_eq!(answer["fee_estimate"], json!("400.000000"));
        assert_eq!(answer["risk_assessment"], json!("medium"));
        assert_eq!(answer["status"], json!("issued"));
        assert_eq!(
            answer["documentation_required"].as_array().unwrap().len(),
            6
        );
        assert_eq!(
            answer["documentation_required"][5],
            json!("Proforma invoice")
        );
    }

    #[test]
    fn factoring_is_the_dearest_and_an_advance_payment_the_cheapest() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "i-1", "instrument_type": "factoring", "amount": "10000.000000"})],
            vec![
                json!({"id": "i-2", "instrument_type": "advance_payment", "amount": "10000.000000"}),
            ],
        ]);
        let factoring = create_instrument(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "instrument_type": "factoring",
                    "counterparty": "Bank", "amount": "10000"}),
        )
        .expect("created");
        let advance = create_instrument(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "instrument_type": "advance_payment",
                    "counterparty": "Bank", "amount": "10000"}),
        )
        .expect("created");
        assert_eq!(factoring["fee_estimate"], json!("300.000000"));
        assert_eq!(advance["fee_estimate"], json!("0.000000"));
        // only a letter of credit asks for the extra two papers
        assert_eq!(
            advance["documentation_required"].as_array().unwrap().len(),
            4
        );
    }

    #[test]
    fn the_risk_band_follows_the_face_amount() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "i-1", "instrument_type": "bank_guarantee", "amount": "99999.990000"}),
            ],
            vec![
                json!({"id": "i-2", "instrument_type": "bank_guarantee", "amount": "500000.000000"}),
            ],
        ]);
        let small = create_instrument(
            &graph,
            &scope(),
            &json!({"instrument_type": "bank_guarantee", "amount": "99999.99"}),
        )
        .expect("created");
        let large = create_instrument(
            &graph,
            &scope(),
            &json!({"instrument_type": "bank_guarantee", "amount": "500000"}),
        )
        .expect("created");
        assert_eq!(small["risk_assessment"], json!("low"));
        assert_eq!(large["risk_assessment"], json!("high"));
    }

    #[test]
    fn an_instrument_type_the_service_does_not_know_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_instrument(
            &graph,
            &scope(),
            &json!({"instrument_type": "promissory_note", "amount": "10"}),
        )
        .is_err());
    }

    #[test]
    fn the_listing_reads_oldest_first_and_narrows_by_status() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "i-2", "company_id": "c-1", "issue_date": "2026-03-01", "status": "issued"}),
            json!({"id": "i-1", "company_id": "c-1", "issue_date": "2026-01-01", "status": "paid"}),
            json!({"id": "i-3", "company_id": "c-2", "issue_date": "2026-01-01", "status": "issued"}),
        ]);
        let answer =
            list_instruments(&graph, &scope(), &json!({"company_id": "c-1"})).expect("listed");
        assert_eq!(answer[0]["id"], json!("i-1"));

        let issued = list_instruments(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "status": "issued"}),
        )
        .expect("listed");
        assert_eq!(issued.as_array().unwrap().len(), 1);
        assert_eq!(issued[0]["id"], json!("i-2"));
    }

    #[test]
    fn an_instrument_the_company_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            present_documents(&graph, &scope(), "i-9", &json!({"company_id": "c-1"})),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn settling_moves_the_status_and_answers_with_the_face_amount() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "i-1", "company_id": "c-1", "amount": "40000.000000",
                        "status": "presented"}),
            ],
            vec![json!({"id": "i-1", "status": "paid"})],
        ]);
        let answer = settle_instrument(&graph, &scope(), "i-1", &json!({"company_id": "c-1"}))
            .expect("settled");
        assert_eq!(answer["status"], json!("paid"));
        assert_eq!(answer["amount"], json!("40000.000000"));
    }
}
