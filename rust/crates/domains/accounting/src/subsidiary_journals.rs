#![forbid(unsafe_code)]
//! accounting-service's books of original entry: the subsidiary journals and the vendor bill.
//!
//! Each of these records a business document and posts the double entry it implies, in one act, so the
//! journal record and the ledger entry it generated agree by construction: the record carries the id of
//! the entry it posted. Amounts are exact.
//!
//! The accounts each journal posts to are the service's own fixed chart, which is the first thing to
//! know about these routes: a sales invoice posts to `1100` and `4000`, a purchase to `1200` and `2000`,
//! and so on. Nothing here derives an account from the document, so a book whose chart lacks those
//! numbers refuses the document rather than guessing where it belongs. That is pinned.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * tax is recorded on the document but never posted: the sales and purchases journals post the total
//!   less the discount, so a taxed invoice reaches the ledger without its tax, and the returns journals
//!   post the total less the tax;
//! * a purchase return debits the purchases-returns account and credits payables, which raises an expense
//!   account instead of lowering the one it reverses;
//! * a cash receipt credits revenue for a cash sale and payables-clearing revenue `4100` for anything
//!   else, so a customer's payment lands in a revenue account rather than against receivables;
//! * the accounts-receivable and accounts-payable ledgers take `balance_due` to be the document total and
//!   `amount_paid` to be nothing, because no payments are tracked against either;
//! * both call a document overdue once it is more than thirty days old, whatever its recorded status;
//! * the fixed-assets and inventory subsidiary ledgers answer with nothing at all: they were placeholders
//!   in the service and nothing here invents figures for them.
//!
//! The shipped receivable and payable ledgers filtered on a date belonging to a node their query never
//! matched, so the filter did nothing. The port filters on the document's own date, which is what it meant.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::{amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CASH_DISBURSEMENTS_JOURNAL, CASH_RECEIPTS_JOURNAL,
    PURCHASES_JOURNAL, PURCHASES_RETURNS_JOURNAL, SALES_JOURNAL, SALES_RETURNS_JOURNAL,
    VENDOR_BILL,
};

use crate::ledger_book;
use vimbai_store::read::{field, money, money_or_nothing, optional, or_default, refused, text};
use vimbai_store::write::remember;

/// Posts a document's double entry and returns the entry's id.
#[allow(clippy::too_many_arguments)] // one argument per posting decision, mirroring the document
fn post(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    description: String,
    reference: &str,
    source_module: &str,
    debit_account: &str,
    credit_account: &str,
    amount: &Money,
    line_note: &str,
) -> Result<String, StoreError> {
    let zero = Money::zero();
    let entry = json!({
        "entry_date": text(payload, "entry_date"),
        "description": description,
        "reference_number": reference,
        "source_module": source_module,
        "status": "posted",
        "created_at": text(payload, "created_at"),
        "lines": [
            {"account_number": debit_account, "debit": wire(amount), "credit": wire(&zero),
             "description": line_note},
            {"account_number": credit_account, "debit": wire(&zero), "credit": wire(amount),
             "description": line_note},
        ],
    });
    let posted = ledger_book::create_entry(graph, scope, &entry)?;
    Ok(field(&posted, "id").to_string())
}

/// The listing filters the journals share: a date field and, where the service offered it, an owner
/// and a status.
fn listing(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    query: &Value,
    date_key: &str,
    owner_key: Option<(&str, &str)>,
) -> Result<Value, StoreError> {
    let from = optional(query, "start_date");
    let until = optional(query, "end_date");
    let status = optional(query, "status");
    let mut rows: Vec<Value> = Records::new(graph, kind)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            from.as_deref()
                .map(|b| field(row, date_key) >= b)
                .unwrap_or(true)
        })
        .filter(|row| {
            until
                .as_deref()
                .map(|b| field(row, date_key) <= b)
                .unwrap_or(true)
        })
        .filter(|row| {
            owner_key
                .and_then(|(incoming, stored)| {
                    optional(query, incoming).map(|wanted| field(row, stored) == wanted)
                })
                .unwrap_or(true)
        })
        .filter(|row| {
            status
                .as_deref()
                .map(|wanted| field(row, "status") == wanted)
                .unwrap_or(true)
        })
        .collect();
    rows.sort_by(|one, two| {
        field(two, date_key)
            .cmp(field(one, date_key))
            .then(field(two, "created_at").cmp(field(one, "created_at")))
    });
    Ok(Value::Array(rows))
}

/// `POST /sales-journal/`: a credit sale, posted to receivables and sales.
pub fn create_sale(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = money(payload, "total_amount")?;
    let discount = money_or_nothing(payload, "discount_amount")?;
    let tax = money_or_nothing(payload, "tax_amount")?;
    let net = total.minus(&discount);
    let invoice = text(payload, "invoice_number");

    let entry_id = post(
        graph,
        scope,
        payload,
        format!("Sales Journal: Invoice {invoice}"),
        &invoice,
        "SalesJournal",
        "1100",
        "4000",
        &net,
        &format!("Sales Invoice {invoice}"),
    )?;

    remember(
        graph,
        scope,
        SALES_JOURNAL,
        json!({
            "invoice_number": invoice,
            "customer_id": text(payload, "customer_id"),
            "invoice_date": text(payload, "entry_date"),
            "due_date": payload.get("due_date").cloned().unwrap_or(Value::Null),
            "total_amount": wire(&total),
            "tax_amount": wire(&tax),
            "discount_amount": wire(&discount),
            "currency": or_default(payload, "currency", "USD"),
            "status": or_default(payload, "status", "pending"),
            "notes": text(payload, "notes"),
            "journal_entry_id": entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /sales-journal/`.
pub fn list_sales(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    listing(
        graph,
        scope,
        SALES_JOURNAL,
        query,
        "invoice_date",
        Some(("customer_id", "customer_id")),
    )
}

/// `GET /sales-journal/{id}`.
pub fn get_sale(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    entry_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, SALES_JOURNAL).get(scope, entry_id)
}

/// `POST /purchases-journal/`: a credit purchase, posted to inventory and payables.
pub fn create_purchase(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = money(payload, "total_amount")?;
    let discount = money_or_nothing(payload, "discount_amount")?;
    let tax = money_or_nothing(payload, "tax_amount")?;
    let net = total.minus(&discount);
    let order = text(payload, "purchase_order_number");

    let entry_id = post(
        graph,
        scope,
        payload,
        format!("Purchases Journal: PO {order}"),
        &order,
        "PurchasesJournal",
        "1200",
        "2000",
        &net,
        &format!("Purchase {order}"),
    )?;

    remember(
        graph,
        scope,
        PURCHASES_JOURNAL,
        json!({
            "purchase_order_number": order,
            "vendor_id": text(payload, "vendor_id"),
            "purchase_date": text(payload, "entry_date"),
            "due_date": payload.get("due_date").cloned().unwrap_or(Value::Null),
            "total_amount": wire(&total),
            "tax_amount": wire(&tax),
            "discount_amount": wire(&discount),
            "currency": or_default(payload, "currency", "USD"),
            "status": or_default(payload, "status", "pending"),
            "notes": text(payload, "notes"),
            "journal_entry_id": entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /purchases-journal/`.
pub fn list_purchases(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    listing(
        graph,
        scope,
        PURCHASES_JOURNAL,
        query,
        "purchase_date",
        Some(("vendor_id", "vendor_id")),
    )
}

/// `POST /cash-receipts-journal/`: money in, posted against a revenue account chosen by source type.
pub fn create_receipt(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = money(payload, "amount")?;
    let receipt = text(payload, "receipt_number");
    let cash_account = or_default(payload, "bank_account", "1000");
    // a cash sale earns revenue; everything else was posted to the other revenue account as shipped
    let revenue_account = if text(payload, "source_type") == "cash_sale" {
        "4000"
    } else {
        "4100"
    };

    let entry_id = post(
        graph,
        scope,
        payload,
        format!("Cash Receipts: {receipt}"),
        &receipt,
        "CashReceiptsJournal",
        &cash_account,
        revenue_account,
        &amount,
        &text(payload, "description"),
    )?;

    remember(
        graph,
        scope,
        CASH_RECEIPTS_JOURNAL,
        json!({
            "receipt_number": receipt,
            "customer_id": payload.get("customer_id").cloned().unwrap_or(Value::Null),
            "receipt_date": text(payload, "entry_date"),
            "amount": wire(&amount),
            "payment_method": or_default(payload, "payment_method", "cash"),
            "reference_number": payload.get("reference_number").cloned().unwrap_or(Value::Null),
            "bank_account": cash_account,
            "description": text(payload, "description"),
            "source_type": or_default(payload, "source_type", "other"),
            "status": or_default(payload, "status", "posted"),
            "journal_entry_id": entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /cash-receipts-journal/`.
pub fn list_receipts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    listing(
        graph,
        scope,
        CASH_RECEIPTS_JOURNAL,
        query,
        "receipt_date",
        None,
    )
}

/// `POST /cash-disbursements-journal/`: money out, posted to the expense account the caller names.
pub fn create_disbursement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = money(payload, "amount")?;
    let payment = text(payload, "payment_number");
    let cash_account = or_default(payload, "bank_account", "1000");
    let expense_account = text(payload, "expense_account");
    if expense_account.is_empty() {
        return Err(refused("expense_account is required"));
    }

    let entry_id = post(
        graph,
        scope,
        payload,
        format!("Cash Disbursement: {payment}"),
        &payment,
        "CashDisbursementsJournal",
        &expense_account,
        &cash_account,
        &amount,
        &text(payload, "description"),
    )?;

    remember(
        graph,
        scope,
        CASH_DISBURSEMENTS_JOURNAL,
        json!({
            "payment_number": payment,
            "vendor_id": payload.get("vendor_id").cloned().unwrap_or(Value::Null),
            "employee_id": payload.get("employee_id").cloned().unwrap_or(Value::Null),
            "payment_date": text(payload, "entry_date"),
            "amount": wire(&amount),
            "payment_method": or_default(payload, "payment_method", "check"),
            "reference_number": payload.get("reference_number").cloned().unwrap_or(Value::Null),
            "bank_account": cash_account,
            "description": text(payload, "description"),
            "expense_account": expense_account,
            "source_type": or_default(payload, "source_type", "other"),
            "status": or_default(payload, "status", "posted"),
            "journal_entry_id": entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /cash-disbursements-journal/`.
pub fn list_disbursements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    listing(
        graph,
        scope,
        CASH_DISBURSEMENTS_JOURNAL,
        query,
        "payment_date",
        None,
    )
}

/// `POST /sales-returns-journal/`: goods back from a customer, posted against receivables.
pub fn create_sales_return(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = money(payload, "total_amount")?;
    let tax = money_or_nothing(payload, "tax_amount")?;
    let net = total.minus(&tax);
    let reference = text(payload, "return_number");

    let entry_id = post(
        graph,
        scope,
        payload,
        format!("Sales Returns: {reference}"),
        &reference,
        "SalesReturnsJournal",
        "4100",
        "1100",
        &net,
        &format!("Sales Return {reference}"),
    )?;

    remember(
        graph,
        scope,
        SALES_RETURNS_JOURNAL,
        json!({
            "return_number": reference,
            "original_invoice_number": text(payload, "original_invoice_number"),
            "customer_id": text(payload, "customer_id"),
            "return_date": text(payload, "entry_date"),
            "total_amount": wire(&total),
            "tax_amount": wire(&tax),
            "reason": text(payload, "reason"),
            "status": or_default(payload, "status", "pending"),
            "notes": text(payload, "notes"),
            "journal_entry_id": entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `POST /purchases-returns-journal/`: goods back to a supplier. As shipped, the expense side is a debit.
pub fn create_purchases_return(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = money(payload, "total_amount")?;
    let tax = money_or_nothing(payload, "tax_amount")?;
    let net = total.minus(&tax);
    let reference = text(payload, "return_number");

    let entry_id = post(
        graph,
        scope,
        payload,
        format!("Purchases Returns: {reference}"),
        &reference,
        "PurchasesReturnsJournal",
        "5100",
        "2000",
        &net,
        &format!("Purchases Return {reference}"),
    )?;

    remember(
        graph,
        scope,
        PURCHASES_RETURNS_JOURNAL,
        json!({
            "return_number": reference,
            "original_po_number": text(payload, "original_po_number"),
            "vendor_id": text(payload, "vendor_id"),
            "return_date": text(payload, "entry_date"),
            "total_amount": wire(&total),
            "tax_amount": wire(&tax),
            "reason": text(payload, "reason"),
            "status": or_default(payload, "status", "pending"),
            "notes": text(payload, "notes"),
            "journal_entry_id": entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `POST /vendor-bills/`: a bill received, posted to the fixed expense account and payables.
pub fn create_vendor_bill(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = money(payload, "total_amount")?;
    let bill = text(payload, "bill_number");

    let entry_id = post(
        graph,
        scope,
        payload,
        format!("Auto-generated JE for Vendor Bill {bill}"),
        &bill,
        "VendorBilling",
        "6000",
        "2000",
        &total,
        &format!("Vendor Bill {bill}"),
    )?;

    remember(
        graph,
        scope,
        VENDOR_BILL,
        json!({
            "vendor_id": text(payload, "vendor_id"),
            "bill_number": bill,
            "invoice_date": text(payload, "entry_date"),
            "due_date": payload.get("due_date").cloned().unwrap_or(Value::Null),
            "total_amount": wire(&total),
            "currency": or_default(payload, "currency", "USD"),
            "status": or_default(payload, "status", "pending"),
            "description": text(payload, "description"),
            "linked_purchase_order_ids": payload
                .get("linked_purchase_order_ids")
                .cloned()
                .unwrap_or(json!([])),
            "journal_entry_id": entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// Days between two ISO business dates, negative when the later one is first.
///
/// The count itself lives in `vimbai_common::dates`; this keeps the shape the ageing here wants - a date
/// that cannot be read ages nothing rather than stopping the ledger.
fn days_between(later: &str, earlier: &str) -> i64 {
    vimbai_common::dates::days_between(later, earlier).unwrap_or(0)
}

fn document_status(recorded: &str, days_outstanding: i64) -> String {
    if days_outstanding > 30 {
        "overdue".to_string()
    } else {
        recorded.to_string()
    }
}

/// `GET /subsidiary-ledgers/accounts-receivable/`.
///
/// `today` is the date the ageing is measured from; without it nothing is aged and each document keeps
/// the status it was recorded with.
pub fn receivables_ledger(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let as_of = optional(query, "as_of_date");
    let customer = optional(query, "customer_id");
    let today = optional(query, "today");
    let mut rows = Vec::new();
    let mut total_invoice = Money::zero();
    let mut total_balance = Money::zero();
    let mut customers: Vec<String> = Vec::new();
    let mut overdue = 0;

    for record in Records::new(graph, SALES_JOURNAL).list(scope, 2000)? {
        let invoice_date = field(&record, "invoice_date").to_string();
        if as_of
            .as_deref()
            .map(|b| invoice_date.as_str() > b)
            .unwrap_or(false)
        {
            continue;
        }
        if customer
            .as_deref()
            .map(|wanted| field(&record, "customer_id") != wanted)
            .unwrap_or(false)
        {
            continue;
        }
        let amount: Money = amount(&json!({"amount": field(&record, "total_amount")}), "amount")?;
        let days = today
            .as_deref()
            .map(|now| days_between(now, &invoice_date))
            .unwrap_or(0);
        let status = document_status(field(&record, "status"), days);
        if status == "overdue" {
            overdue += 1;
        }
        let payer = field(&record, "customer_id").to_string();
        if !customers.contains(&payer) {
            customers.push(payer.clone());
        }
        total_invoice = total_invoice.plus(&amount);
        total_balance = total_balance.plus(&amount);
        rows.push(json!({
            "customer_id": payer,
            // the service labelled customers by a slice of their id and nothing else
            "customer_name": format!("Customer {}", payer.get(0..8).unwrap_or(&payer)),
            "invoice_number": field(&record, "invoice_number"),
            "invoice_date": invoice_date,
            "due_date": record.get("due_date").cloned().unwrap_or(Value::Null),
            "invoice_amount": wire(&amount),
            "balance_due": wire(&amount),
            "amount_paid": wire(&Money::zero()),
            "status": status,
            "days_outstanding": days,
        }));
    }
    rows.sort_by(|one, two| field(two, "invoice_date").cmp(field(one, "invoice_date")));

    Ok(json!({
        "as_of_date": as_of.unwrap_or_default(),
        "entries": rows,
        "total_invoice_amount": wire(&total_invoice),
        "total_balance_due": wire(&total_balance),
        "total_amount_paid": wire(&Money::zero()),
        "customer_count": customers.len(),
        "overdue_count": overdue,
    }))
}

/// `GET /subsidiary-ledgers/accounts-payable/`.
pub fn payables_ledger(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let as_of = optional(query, "as_of_date");
    let vendor = optional(query, "vendor_id");
    let today = optional(query, "today");
    let mut rows = Vec::new();
    let mut total_bill = Money::zero();
    let mut total_balance = Money::zero();
    let mut vendors: Vec<String> = Vec::new();
    let mut overdue = 0;

    for record in Records::new(graph, PURCHASES_JOURNAL).list(scope, 2000)? {
        let bill_date = field(&record, "purchase_date").to_string();
        if as_of
            .as_deref()
            .map(|b| bill_date.as_str() > b)
            .unwrap_or(false)
        {
            continue;
        }
        if vendor
            .as_deref()
            .map(|wanted| field(&record, "vendor_id") != wanted)
            .unwrap_or(false)
        {
            continue;
        }
        let amount: Money = amount(&json!({"amount": field(&record, "total_amount")}), "amount")?;
        let days = today
            .as_deref()
            .map(|now| days_between(now, &bill_date))
            .unwrap_or(0);
        let status = document_status(field(&record, "status"), days);
        if status == "overdue" {
            overdue += 1;
        }
        let payee = field(&record, "vendor_id").to_string();
        if !vendors.contains(&payee) {
            vendors.push(payee.clone());
        }
        total_bill = total_bill.plus(&amount);
        total_balance = total_balance.plus(&amount);
        rows.push(json!({
            "vendor_id": payee,
            "vendor_name": format!("Vendor {}", payee.get(0..8).unwrap_or(&payee)),
            "bill_number": field(&record, "purchase_order_number"),
            "bill_date": bill_date,
            "due_date": record.get("due_date").cloned().unwrap_or(Value::Null),
            "bill_amount": wire(&amount),
            "balance_due": wire(&amount),
            "amount_paid": wire(&Money::zero()),
            "status": status,
            "days_outstanding": days,
        }));
    }
    rows.sort_by(|one, two| field(two, "bill_date").cmp(field(one, "bill_date")));

    Ok(json!({
        "as_of_date": as_of.unwrap_or_default(),
        "entries": rows,
        "total_bill_amount": wire(&total_bill),
        "total_balance_due": wire(&total_balance),
        "total_amount_paid": wire(&Money::zero()),
        "vendor_count": vendors.len(),
        "overdue_count": overdue,
    }))
}

/// `GET /subsidiary-ledgers/fixed-assets/`: nothing, exactly as the service answered.
pub fn fixed_assets_ledger(query: &Value) -> Value {
    json!({
        "as_of_date": text(query, "as_of_date"),
        "entries": [],
        "total_purchase_cost": wire(&Money::zero()),
        "total_accumulated_depreciation": wire(&Money::zero()),
        "total_net_book_value": wire(&Money::zero()),
        "asset_count": 0,
    })
}

/// `GET /subsidiary-ledgers/inventory/`: nothing, exactly as the service answered.
pub fn inventory_ledger(query: &Value) -> Value {
    json!({
        "as_of_date": text(query, "as_of_date"),
        "entries": [],
        "total_opening_value": wire(&Money::zero()),
        "total_stock_in_value": wire(&Money::zero()),
        "total_stock_out_value": wire(&Money::zero()),
        "total_closing_value": wire(&Money::zero()),
        "item_count": 0,
        "low_stock_count": 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn chart() -> Vec<Value> {
        [
            ("1000", "asset", "debit"),
            ("1100", "asset", "debit"),
            ("1200", "asset", "debit"),
            ("2000", "liability", "credit"),
            ("4000", "revenue", "credit"),
            ("4100", "revenue", "credit"),
            ("5100", "expense", "debit"),
            ("6000", "expense", "debit"),
        ]
        .into_iter()
        .map(|(number, kind, normal)| {
            json!({"id": format!("acct-{number}"), "account_number": number,
                   "name": format!("{kind} {number}"), "account_type": kind,
                   "normal_balance": normal})
        })
        .collect()
    }

    /// Runs a create against a chart and returns the journal entry that was posted.
    fn posted_entry(build: impl FnOnce(&FakeGraph) -> Result<Value, StoreError>) -> Value {
        // two chart reads (one per line), then the chain, then the entry write, then the journal write
        let graph = FakeGraph::queue(vec![
            chart(),
            chart(),
            vec![],
            vec![json!({"id": "je-1"})],
            vec![json!({"id": "doc-1"})],
        ]);
        build(&graph).expect("created");
        let (statement, params) = graph.statement(3);
        assert!(statement.contains("CREATE"), "{statement}");
        params["props"].clone()
    }

    #[test]
    fn a_sales_invoice_posts_the_total_less_discount_and_never_the_tax() {
        let entry = posted_entry(|graph| {
            create_sale(
                graph,
                &scope(),
                &json!({"invoice_number": "INV-1", "customer_id": "cust-1",
                        "entry_date": "2026-03-15", "total_amount": "1000.00",
                        "discount_amount": "50.00", "tax_amount": "140.00"}),
            )
        });
        let lines = entry["lines"].as_array().expect("lines");
        assert_eq!(lines[0]["debit"], json!("950.000000"));
        assert_eq!(lines[0]["account_number"], json!("1100"));
        assert_eq!(lines[1]["credit"], json!("950.000000"));
        // the tax is on the document, not in the entry
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn a_purchase_return_debits_its_expense_account_as_shipped() {
        let entry = posted_entry(|graph| {
            create_purchases_return(
                graph,
                &scope(),
                &json!({"return_number": "PR-1", "vendor_id": "v-1", "entry_date": "2026-03-15",
                        "total_amount": "120.00", "tax_amount": "20.00"}),
            )
        });
        let lines = entry["lines"].as_array().expect("lines");
        // the expense account is debited, which raises it rather than reversing the purchase
        assert_eq!(lines[0]["account_number"], json!("5100"));
        assert_eq!(lines[0]["debit"], json!("100.000000"));
    }

    #[test]
    fn a_cash_sale_earns_revenue_and_anything_else_lands_in_the_other_revenue_account() {
        for (source, account) in [("cash_sale", "4000"), ("customer_payment", "4100")] {
            let entry = posted_entry(|graph| {
                create_receipt(
                    graph,
                    &scope(),
                    &json!({"receipt_number": "R-1", "entry_date": "2026-03-15",
                            "amount": "75.50", "source_type": source, "description": "cash in"}),
                )
            });
            let lines = entry["lines"].as_array().expect("lines");
            assert_eq!(lines[0]["account_number"], json!("1000"));
            assert_eq!(lines[1]["account_number"], json!(account));
            assert_eq!(lines[1]["credit"], json!("75.500000"));
        }
    }

    #[test]
    fn a_disbursement_needs_the_expense_account_it_was_told() {
        let graph = FakeGraph::returning(chart());
        let refused = create_disbursement(
            &graph,
            &scope(),
            &json!({"payment_number": "P-1", "amount": "10.00", "description": "stamps"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn an_amount_no_float_could_hold_survives_the_document_and_the_entry() {
        let exact = "123456789012345678901.55";
        let entry = posted_entry(|graph| {
            create_sale(
                graph,
                &scope(),
                &json!({"invoice_number": "INV-9", "customer_id": "c-1",
                        "entry_date": "2026-03-15", "total_amount": exact}),
            )
        });
        assert_eq!(
            entry["lines"][0]["debit"],
            json!("123456789012345678901.550000")
        );
    }

    #[test]
    fn a_sales_invoice_is_refused_when_the_fixed_chart_is_missing() {
        // only a cash account: the journal's own 1100 and 4000 do not exist
        let graph = FakeGraph::returning(vec![json!({
            "id": "acct-1000", "account_number": "1000", "name": "Cash",
            "account_type": "asset", "normal_balance": "debit"})]);
        let refused = create_sale(
            &graph,
            &scope(),
            &json!({"invoice_number": "INV-1", "customer_id": "c-1", "entry_date": "2026-03-15",
                    "total_amount": "10.00"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn the_receivable_ledger_owes_the_whole_document_and_ages_it() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "s-1", "invoice_number": "INV-1", "customer_id": "cust-1234567890",
                   "invoice_date": "2026-01-01", "total_amount": "500.000000",
                   "status": "invoiced"}),
            json!({"id": "s-2", "invoice_number": "INV-2", "customer_id": "cust-1234567890",
                   "invoice_date": "2026-03-10", "total_amount": "250.000000",
                   "status": "invoiced"}),
        ]);
        let answer = receivables_ledger(
            &graph,
            &scope(),
            &json!({"today": "2026-03-15", "as_of_date": "2026-03-31"}),
        )
        .expect("receivables");
        // the older invoice is past thirty days and called overdue whatever it was recorded as
        assert_eq!(answer["overdue_count"], json!(1));
        assert_eq!(answer["entries"][1]["status"], json!("overdue"));
        assert_eq!(answer["entries"][1]["days_outstanding"], json!(73));
        assert_eq!(answer["entries"][0]["status"], json!("invoiced"));
        assert_eq!(answer["total_balance_due"], json!("750.000000"));
        assert_eq!(answer["total_amount_paid"], json!("0.000000"));
        assert_eq!(answer["customer_count"], json!(1));
    }

    #[test]
    fn the_receivable_ledger_leaves_out_documents_after_its_date() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "s-1", "invoice_number": "INV-1", "customer_id": "c-1",
                   "invoice_date": "2026-01-01", "total_amount": "100.000000"}),
            json!({"id": "s-2", "invoice_number": "INV-2", "customer_id": "c-1",
                   "invoice_date": "2026-04-01", "total_amount": "999.000000"}),
        ]);
        let answer = receivables_ledger(&graph, &scope(), &json!({"as_of_date": "2026-03-31"}))
            .expect("rows");
        assert_eq!(answer["entries"].as_array().expect("entries").len(), 1);
        assert_eq!(answer["total_invoice_amount"], json!("100.000000"));
    }

    #[test]
    fn the_payable_ledger_reads_the_purchases_journal() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "p-1", "purchase_order_number": "PO-1", "vendor_id": "vendor-abcdefgh",
            "purchase_date": "2026-03-01", "total_amount": "400.000000", "status": "received"})]);
        let answer =
            payables_ledger(&graph, &scope(), &json!({"today": "2026-03-10"})).expect("rows");
        assert_eq!(answer["entries"][0]["bill_number"], json!("PO-1"));
        assert_eq!(answer["entries"][0]["days_outstanding"], json!(9));
        assert_eq!(answer["total_bill_amount"], json!("400.000000"));
        assert_eq!(answer["vendor_count"], json!(1));
    }

    #[test]
    fn the_placeholder_ledgers_still_answer_with_nothing() {
        let assets = fixed_assets_ledger(&json!({"as_of_date": "2026-03-31"}));
        assert_eq!(assets["asset_count"], json!(0));
        assert_eq!(assets["total_net_book_value"], json!("0.000000"));
        let inventory = inventory_ledger(&json!({}));
        assert_eq!(inventory["item_count"], json!(0));
        assert_eq!(inventory["low_stock_count"], json!(0));
    }

    #[test]
    fn days_between_counts_across_months_and_years() {
        assert_eq!(days_between("2026-03-15", "2026-03-01"), 14);
        assert_eq!(days_between("2026-03-01", "2026-01-31"), 29);
        assert_eq!(days_between("2027-01-01", "2026-01-01"), 365);
        // a leap day is one day
        assert_eq!(days_between("2028-03-01", "2028-02-29"), 1);
    }
}
