#![forbid(unsafe_code)]
//! invoicing-service: the customers a Book bills, the invoices it raises, and what has been paid against them.
//!
//! Three rules from the shipped service decide whether an invoice register can be trusted, and all three are kept:
//!
//! * a customer identifier is the Book's own, and a second customer under the same identifier is refused. The identifier
//!   is what invoices point at, so two customers sharing one would silently split a debtor's history;
//! * an invoice must belong to a customer that exists here, and its number must not already be taken. A number is how an
//!   invoice is addressed everywhere else in the Book, so a duplicate makes every later lookup ambiguous;
//! * an invoice's total must match the sum of its items, to the cent. The shipped service allowed exactly one cent of
//!   drift and refused anything further, and that tolerance is kept rather than tightened: a rounding difference of a
//!   cent between a total and its lines is real bookkeeping, and refusing it would block invoices the Book already holds.
//!
//! A due date before the invoice date is refused, because an invoice that was overdue the moment it was raised is a data
//! error rather than a payment problem.
//!
//! Recording a payment is done here, against the invoice: the amount is added to what has been paid, and the invoice
//! becomes paid once it is settled. ONE DELIBERATE DIFFERENCE. The shipped service reached out to the accounting service
//! with the caller's token and asked it to post a journal entry. That would make this crate a second route into the
//! ledger, and the ledger has one: a payment posts through accounting, where the entries are written once and corrected
//! only by reversing entries. So the payment is recorded against the invoice and the answer says plainly that no journal
//! entry was posted, rather than reporting a posting that did not happen.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_common::Money;
use vimbai_store::amounts::{amount, amount_or_zero, wire};
use vimbai_store::read::text;
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A customer the Book bills.
pub const CUSTOMER: RecordKind = RecordKind::new("InvoiceCustomer", "OWNS_INVOICE_CUSTOMER");
/// An invoice raised against a customer.
pub const INVOICE: RecordKind = RecordKind::new("Invoice", "OWNS_INVOICE");
/// A payment recorded against an invoice.
pub const PAYMENT: RecordKind = RecordKind::new("InvoicePayment", "OWNS_INVOICE_PAYMENT");

/// The five states an invoice can be in. Anything else is refused by name.
pub const STATUSES: [&str; 5] = ["draft", "outstanding", "paid", "overdue", "void"];

/// One cent, the drift a total may have from its lines.
const CENT: &str = "0.01";

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

fn status_of(payload: &Value, fallback: &str) -> Result<String, StoreError> {
    match text(payload, "status") {
        named if named.is_empty() => Ok(fallback.to_string()),
        named if STATUSES.contains(&named.as_str()) => Ok(named),
        other => Err(StoreError::BadRequest(format!(
            "Invalid status {other:?}. Must be one of {STATUSES:?}"
        ))),
    }
}

/// The sum of an invoice's lines.
fn lines_total(items: &[Value]) -> Result<Money, StoreError> {
    let mut total = Money::zero();
    for item in items {
        total = total.plus(&amount_or_zero(item, "amount")?);
    }
    Ok(total)
}

/// Check a total against its lines, allowing the one cent the shipped service allowed.
fn check_total(stated: &Money, items: &[Value]) -> Result<(), StoreError> {
    let lines = lines_total(items)?;
    let drift = stated.minus(&lines);
    let absolute = if drift.is_negative() {
        drift.times(&Money::from_i64(-1))
    } else {
        drift.clone()
    };
    if absolute > amount(&json!({"drift": CENT}), "drift")? {
        return Err(StoreError::BadRequest(format!(
            "Total amount ({}) does not match sum of item amounts ({})",
            stated.to_fixed(2),
            lines.to_fixed(2)
        )));
    }
    Ok(())
}

/// `POST /customers`: register a customer.
pub fn create_invoice_customer(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "customer_id");
    if wanted.is_empty() {
        return Err(StoreError::BadRequest("customer_id is required".into()));
    }
    // the identifier is what invoices point at, so two customers may not share one
    let records = Records::new(graph, CUSTOMER);
    if records
        .list(scope, 500)?
        .iter()
        .any(|row| text(row, "customer_id") == wanted)
    {
        return Err(StoreError::Conflict(
            "Customer with this ID already exists for user.".into(),
        ));
    }
    records.create(
        scope,
        &json!({
            "id": service_id(),
            "customer_id": wanted,
            "name": text(payload, "name"),
            "email": text(payload, "email"),
            "phone": text(payload, "phone"),
            "address": text(payload, "address"),
            "created_at": stamp(payload),
            "updated_at": stamp(payload),
        }),
    )
}

/// `GET /customers`: the Book's customers.
pub fn invoice_customers(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, CUSTOMER).list(scope, 500)?,
    ))
}

/// `GET /customers/{customer_id}`: one customer.
pub fn invoice_customer(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    customer_id: &str,
) -> Result<Value, StoreError> {
    let wanted = customer_id.to_string();
    Records::new(graph, CUSTOMER)
        .list(scope, 500)?
        .into_iter()
        .find(|row| text(row, "customer_id") == wanted)
        .ok_or(StoreError::NotFound)
}

/// `PUT /customers/{customer_id}`: change the fields the caller named, and only those.
pub fn update_invoice_customer(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    customer_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = invoice_customer(graph, scope, customer_id)?;
    let mut change = json!({"updated_at": stamp(payload)});
    for field in ["name", "email", "phone", "address"] {
        if payload.get(field).is_some() {
            change[field] = json!(text(payload, field));
        }
    }
    Records::new(graph, CUSTOMER).update(scope, &text(&held, "id"), &change)
}

/// `DELETE /customers/{customer_id}`: forget a customer.
pub fn delete_invoice_customer(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    customer_id: &str,
) -> Result<Value, StoreError> {
    let held = invoice_customer(graph, scope, customer_id)?;
    Records::new(graph, CUSTOMER).delete(scope, &text(&held, "id"))?;
    Ok(json!({"ok": true}))
}

/// The invoice held under a number, if this Book has one.
fn held_invoice(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    invoice_number: &str,
) -> Result<Option<Value>, StoreError> {
    let wanted = invoice_number.to_string();
    Ok(Records::new(graph, INVOICE)
        .list(scope, 500)?
        .into_iter()
        .find(|row| text(row, "invoice_number") == wanted))
}

/// `POST /invoices`: raise an invoice.
pub fn create_invoice(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    // an invoice bills a customer that exists here
    let customer_id = text(payload, "customer_id");
    invoice_customer(graph, scope, &customer_id)
        .map_err(|_| StoreError::BadRequest("Customer not found.".into()))?;

    let invoice_number = text(payload, "invoice_number");
    if held_invoice(graph, scope, &invoice_number)?.is_some() {
        return Err(StoreError::Conflict(
            "Invoice with this number already exists.".into(),
        ));
    }

    let items = payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Err(StoreError::BadRequest(
            "items must hold at least one line".into(),
        ));
    }
    let total = amount_or_zero(payload, "total_amount")?;
    check_total(&total, &items)?;

    // an invoice that was overdue the moment it was raised is a data error, not a payment problem
    let raised = text(payload, "invoice_date");
    let due = text(payload, "due_date");
    if !raised.is_empty() && !due.is_empty() && due.as_str() < raised.as_str() {
        return Err(StoreError::BadRequest(
            "Due date cannot be before invoice date.".into(),
        ));
    }

    let mut lines = Vec::with_capacity(items.len());
    for item in &items {
        lines.push(json!({
            "id": service_id(),
            "description": text(item, "description"),
            "quantity": wire(&amount_or_zero(item, "quantity")?),
            "unit_price": wire(&amount_or_zero(item, "unit_price")?),
            "amount": wire(&amount_or_zero(item, "amount")?),
        }));
    }

    let status = status_of(payload, "draft")?;
    Records::new(graph, INVOICE).create(
        scope,
        &json!({
            "id": service_id(),
            "invoice_number": invoice_number,
            "customer_id": customer_id,
            "invoice_date": if raised.is_empty() { stamp(payload) } else { json!(raised) },
            "due_date": json!(due),
            "items": lines,
            "total_amount": wire(&total),
            "paid_amount": wire(&Money::zero()),
            "status": status,
            "notes": text(payload, "notes"),
            "created_at": stamp(payload),
            "updated_at": stamp(payload),
        }),
    )
}

/// `GET /invoices`: the Book's invoices.
pub fn invoices(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(Records::new(graph, INVOICE).list(scope, 500)?))
}

/// `GET /invoices/{invoice_number}`: one invoice.
pub fn invoice(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    invoice_number: &str,
) -> Result<Value, StoreError> {
    held_invoice(graph, scope, invoice_number)?.ok_or(StoreError::NotFound)
}

/// `PUT /invoices/{invoice_number}`: change the fields the caller named, and only those.
///
/// A new total is checked against the lines that stand, so an invoice cannot be re-totalled away from what it bills.
pub fn update_invoice(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    invoice_number: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = invoice(graph, scope, invoice_number)?;
    let items = match payload.get("items").and_then(Value::as_array) {
        Some(named) => named.clone(),
        None => held
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    let mut change = json!({"updated_at": stamp(payload)});
    if payload.get("items").is_some() {
        change["items"] = json!(items);
    }
    if payload.get("total_amount").is_some() {
        let total = amount_or_zero(payload, "total_amount")?;
        check_total(&total, &items)?;
        change["total_amount"] = json!(wire(&total));
    }
    if payload.get("status").is_some() {
        change["status"] = json!(status_of(payload, "draft")?);
    }
    for field in ["due_date", "notes"] {
        if payload.get(field).is_some() {
            change[field] = json!(text(payload, field));
        }
    }
    Records::new(graph, INVOICE).update(scope, &text(&held, "id"), &change)
}

/// `DELETE /invoices/{invoice_number}`: forget an invoice.
pub fn delete_invoice(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    invoice_number: &str,
) -> Result<Value, StoreError> {
    let held = invoice(graph, scope, invoice_number)?;
    Records::new(graph, INVOICE).delete(scope, &text(&held, "id"))?;
    Ok(json!({"ok": true}))
}

/// `POST /invoices/{invoice_number}/record-payment`: record what was paid against an invoice.
///
/// The payment is kept as its own record, and the invoice carries the running total. An invoice that is fully paid is
/// marked paid; one that is partly paid keeps its status as the caller set it, because a part payment does not settle a
/// bill. No journal entry is posted here - the answer says so rather than reporting a posting that did not happen.
pub fn record_invoice_payment(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    invoice_number: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = invoice(graph, scope, invoice_number)?;
    let paid = amount(payload, "payment_amount")?;
    if paid.is_zero() || paid.is_negative() {
        return Err(StoreError::BadRequest(
            "payment_amount must be more than nothing".into(),
        ));
    }

    Records::new(graph, PAYMENT).create(
        scope,
        &json!({
            "id": service_id(),
            "invoice_number": invoice_number,
            "amount": wire(&paid),
            "payment_date": if text(payload, "payment_date").is_empty() {
                stamp(payload)
            } else {
                json!(text(payload, "payment_date"))
            },
            "created_at": stamp(payload),
        }),
    )?;

    let already = amount_or_zero(&held, "paid_amount")?;
    let now_paid = already.plus(&paid);
    let total = amount_or_zero(&held, "total_amount")?;
    let settled = now_paid >= total;
    let change = if settled {
        json!({
            "paid_amount": wire(&now_paid),
            "status": "paid",
            "updated_at": stamp(payload),
        })
    } else {
        json!({
            "paid_amount": wire(&now_paid),
            "updated_at": stamp(payload),
        })
    };
    let invoice = Records::new(graph, INVOICE).update(scope, &text(&held, "id"), &change)?;

    Ok(json!({
        "invoice_number": invoice_number,
        "payment_amount": wire(&paid),
        "paid_amount": wire(&now_paid),
        "outstanding": wire(&if settled { Money::zero() } else { total.minus(&now_paid) }),
        "status": invoice["status"],
        "journal_posted": false,
        "note": "Recorded against the invoice. A journal entry is posted by the accounting service, which owns the ledger.",
    }))
}

/// `GET /invoices/{invoice_number}/payments`: what has been paid against an invoice.
pub fn invoice_payments(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    invoice_number: &str,
) -> Result<Value, StoreError> {
    let wanted = invoice_number.to_string();
    Ok(Value::Array(
        Records::new(graph, PAYMENT)
            .list(scope, 500)?
            .into_iter()
            .filter(|row| text(row, "invoice_number") == wanted)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn customer(graph: &MemoryGraph) -> Value {
        create_invoice_customer(
            graph,
            &talked(),
            &json!({"customer_id": "ACME-1", "name": "Acme Traders", "email": "billing@acme.test",
                    "now": 1000.0}),
        )
        .expect("a customer")
    }

    fn lines() -> Value {
        json!([
            {"description": "Consulting", "quantity": 2, "unit_price": 50, "amount": 100},
            {"description": "Support", "quantity": 1, "unit_price": 50, "amount": 50}
        ])
    }

    #[test]
    fn a_customer_is_known_by_one_identifier() {
        let graph = MemoryGraph::new();
        let made = customer(&graph);
        assert_eq!(made["customer_id"], json!("ACME-1"));
        assert_eq!(made["name"], json!("Acme Traders"));

        // the identifier is what invoices point at, so a second customer under it is refused
        assert!(matches!(
            create_invoice_customer(
                &graph,
                &talked(),
                &json!({"customer_id": "ACME-1", "name": "Acme Again"})
            ),
            Err(StoreError::Conflict(_))
        ));
        // an identifier nobody stated is refused rather than minted
        assert!(matches!(
            create_invoice_customer(&graph, &talked(), &json!({"name": "Nameless"})),
            Err(StoreError::BadRequest(_))
        ));

        // a change touches only the fields named
        let changed =
            update_invoice_customer(&graph, &talked(), "ACME-1", &json!({"name": "Acme Ltd"}))
                .expect("a change");
        assert_eq!(changed["name"], json!("Acme Ltd"));
        assert_eq!(changed["email"], json!("billing@acme.test"));

        assert!(invoice_customer(&graph, &talked(), "ACME-9").is_err());
        delete_invoice_customer(&graph, &talked(), "ACME-1").expect("a delete");
        assert!(invoice_customer(&graph, &talked(), "ACME-1").is_err());
    }

    #[test]
    fn an_invoice_must_bill_a_customer_and_match_its_lines() {
        let graph = MemoryGraph::new();
        customer(&graph);
        let raised = create_invoice(
            &graph,
            &talked(),
            &json!({"invoice_number": "INV-1", "customer_id": "ACME-1", "invoice_date": "2026-11-01",
                    "due_date": "2026-11-30", "total_amount": 150, "items": lines(), "now": 1000.0}),
        )
        .expect("an invoice");
        assert_eq!(raised["status"], json!("draft"));
        assert_eq!(raised["paid_amount"], json!("0.000000"));
        assert_eq!(raised["total_amount"], json!("150.000000"));
        assert_eq!(raised["items"].as_array().map(Vec::len), Some(2));

        // a customer that does not exist here cannot be billed
        assert!(matches!(
            create_invoice(
                &graph,
                &talked(),
                &json!({"invoice_number": "INV-2", "customer_id": "NOBODY", "total_amount": 150,
                        "items": lines()})
            ),
            Err(StoreError::BadRequest(_))
        ));
        // nor may a number be taken twice
        assert!(matches!(
            create_invoice(
                &graph,
                &talked(),
                &json!({"invoice_number": "INV-1", "customer_id": "ACME-1", "total_amount": 150,
                        "items": lines()})
            ),
            Err(StoreError::Conflict(_))
        ));
        // the total must match its lines, with the one cent of drift the shipped service allowed
        assert!(matches!(
            create_invoice(
                &graph,
                &talked(),
                &json!({"invoice_number": "INV-3", "customer_id": "ACME-1", "total_amount": 200,
                        "items": lines()})
            ),
            Err(StoreError::BadRequest(_))
        ));
        let drifting = create_invoice(
            &graph,
            &talked(),
            &json!({"invoice_number": "INV-4", "customer_id": "ACME-1", "total_amount": "150.01",
                    "items": lines()}),
        )
        .expect("a cent of drift is real bookkeeping");
        assert_eq!(drifting["total_amount"], json!("150.010000"));
        // an invoice that was overdue the moment it was raised is a data error
        assert!(matches!(
            create_invoice(
                &graph,
                &talked(),
                &json!({"invoice_number": "INV-5", "customer_id": "ACME-1", "invoice_date": "2026-11-10",
                        "due_date": "2026-11-01", "total_amount": 150, "items": lines()})
            ),
            Err(StoreError::BadRequest(_))
        ));
        // a sixth state is refused by name
        assert!(matches!(
            create_invoice(
                &graph,
                &talked(),
                &json!({"invoice_number": "INV-6", "customer_id": "ACME-1", "total_amount": 150,
                        "items": lines(), "status": "disputed"})
            ),
            Err(StoreError::BadRequest(_))
        ));
        // no lines, nothing to bill
        assert!(matches!(
            create_invoice(
                &graph,
                &talked(),
                &json!({"invoice_number": "INV-7", "customer_id": "ACME-1", "total_amount": 0, "items": []})
            ),
            Err(StoreError::BadRequest(_))
        ));
    }

    #[test]
    fn a_payment_is_recorded_and_settles_the_invoice_it_covers() {
        let graph = MemoryGraph::new();
        customer(&graph);
        create_invoice(
            &graph,
            &talked(),
            &json!({"invoice_number": "INV-1", "customer_id": "ACME-1", "invoice_date": "2026-11-01",
                    "due_date": "2026-11-30", "total_amount": 150, "items": lines(), "status": "outstanding",
                    "now": 1000.0}),
        )
        .expect("an invoice");

        // part payment stands on the invoice without settling it
        let part = record_invoice_payment(
            &graph,
            &talked(),
            "INV-1",
            &json!({"payment_amount": 50, "payment_date": "2026-11-10", "now": 2000.0}),
        )
        .expect("a payment");
        assert_eq!(part["paid_amount"], json!("50.000000"));
        assert_eq!(part["outstanding"], json!("100.000000"));
        assert_eq!(part["status"], json!("outstanding"));
        // the port is honest that it did not post to the ledger
        assert_eq!(part["journal_posted"], json!(false));

        // the rest settles it
        let rest =
            record_invoice_payment(&graph, &talked(), "INV-1", &json!({"payment_amount": 100}))
                .expect("a payment");
        assert_eq!(rest["paid_amount"], json!("150.000000"));
        assert_eq!(rest["outstanding"], json!("0.000000"));
        assert_eq!(rest["status"], json!("paid"));

        // both payments are kept, and nothing may be paid that is not money
        let kept = invoice_payments(&graph, &talked(), "INV-1").expect("payments");
        assert_eq!(kept.as_array().map(Vec::len), Some(2));
        assert!(matches!(
            record_invoice_payment(&graph, &talked(), "INV-1", &json!({"payment_amount": 0})),
            Err(StoreError::BadRequest(_))
        ));
        assert!(
            record_invoice_payment(&graph, &talked(), "INV-9", &json!({"payment_amount": 5}))
                .is_err()
        );

        // another Book sees none of it
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(invoice(&graph, &elsewhere, "INV-1").is_err());
        assert_eq!(
            invoices(&graph, &elsewhere)
                .expect("invoices")
                .as_array()
                .map(Vec::len),
            Some(0)
        );

        // a re-total is checked against the lines that stand, and a delete removes the invoice
        assert!(matches!(
            update_invoice(&graph, &talked(), "INV-1", &json!({"total_amount": 999})),
            Err(StoreError::BadRequest(_))
        ));
        let voided = update_invoice(
            &graph,
            &talked(),
            "INV-1",
            &json!({"status": "void", "notes": "raised in error"}),
        )
        .expect("a change");
        assert_eq!(voided["status"], json!("void"));
        assert_eq!(voided["notes"], json!("raised in error"));
        delete_invoice(&graph, &talked(), "INV-1").expect("a delete");
        assert!(invoice(&graph, &talked(), "INV-1").is_err());
    }
}
