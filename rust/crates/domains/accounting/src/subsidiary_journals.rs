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
//! Tax is posted as its own line. The service recorded the tax on the document and posted the entry without
//! it, so a Book could show a taxed invoice whose ledger said the business had earned the tax as revenue and
//! owed nobody anything. A taxed sale now posts `Dr receivable (the total the customer pays) / Cr sales (that same
//! figure, so a sale is never understated) / Dr tax expense (each tax, by its own type) / Cr tax owed (each tax, to its
//! own account)`, and a taxed purchase `Dr purchases (net less tax) / Dr tax (tax) / Cr payable (net)`, so
//! collected tax sits as a liability and paid tax as something to reclaim. The tax account is the Book's own:
//! named in the payload, or the one account whose name says VAT or tax. A Book with none is refused by name
//! rather than having its tax posted to nothing, and a tax larger than the document is refused as the typo it
//! is. `GET /accounting/tax` adds what the account holds over a period.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the returns journals post the total less the tax, which is right for a return of a taxed document;
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
use crate::taxes;
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

/// A document's tax cannot be more than the document. A tax larger than the net is not a rate anybody charges; it is a
/// total typed into the tax field, and posting it would leave the sale earning less than nothing.
fn check_tax_fits(tax: &Money, net: &Money, what: &str) -> Result<(), StoreError> {
    if *tax > *net {
        return Err(refused(&format!(
            "The tax on {what} is {} which is more than the {} the document comes to. Check which figure is the tax.",
            tax.to_fixed(2),
            net.to_fixed(2)
        )));
    }
    Ok(())
}

/// The tax account this Book keeps, which is where the tax on a document belongs.
///
/// The caller may name it; otherwise it is the one account in the chart whose name says VAT or tax. A Book with none, or
/// with more than one and no name given, is refused rather than guessed at: putting collected tax in the wrong account
/// hides a liability that somebody has to pay, and inventing a number the chart does not hold would post to nothing.
fn tax_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<String, StoreError> {
    let named = text(payload, "tax_account_number");
    let accounts = crate::ledger_book::accounts(graph, scope)?;
    if !named.is_empty() {
        let held = accounts.iter().any(|account| {
            field(account, "account_number") == named
                && !field(account, "account_number").is_empty()
        });
        if !held {
            return Err(refused(&format!(
                "Account number {named} was given as the tax account but this Book's chart does not hold it."
            )));
        }
        return Ok(named);
    }
    let named_vat: Vec<String> = accounts
        .iter()
        .filter(|account| {
            let name = field(account, "name").to_lowercase();
            let expense = field(account, "account_type") == "expense";
            !expense && (name.contains("vat") || name.contains("tax"))
        })
        .map(|account| field(account, "account_number").to_string())
        .filter(|number| !number.is_empty())
        .collect();
    match named_vat.len() {
        1 => Ok(named_vat[0].clone()),
        0 => Err(refused(
            "This document carries tax, but this Book's chart has no VAT account to hold it. Add one in Chart of \
             accounts, or say which account it is with tax_account_number.",
        )),
        _ => Err(refused(&format!(
            "This Book has {} accounts whose name mentions VAT or tax ({}), so the one to post to is not obvious. Say \
             which with tax_account_number.",
            named_vat.len(),
            named_vat.join(", ")
        ))),
    }
}

/// Posts a document that carries tax: the tax gets its own line, because collected tax is money owed to somebody else
/// and paid tax is money to reclaim, and folding either into income or an expense states both wrongly.
///
/// The total on each side is still the document's net, so the entry balances exactly as it did before.
#[allow(clippy::too_many_arguments)] // one argument per posting decision, mirroring the document
/// One tax on a document: its name, the account it posts to, and how much it is.
type TaxLine = (String, String, String, Money);

/// Which way a taxed document moves, so one posting helper serves a sale, a purchase and their reversals.
///
/// A return is the exact mirror of the document it returns, so it is posted by building the forward lines and turning
/// each one round. That way a change to how a sale posts cannot leave its return posting something else.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TaxedDocument {
    Sale,
    SalesReturn,
    Purchase,
    PurchaseReturn,
}

impl TaxedDocument {
    /// True where the document is money going out to a supplier rather than coming in from a customer.
    fn is_purchase(self) -> bool {
        matches!(self, Self::Purchase | Self::PurchaseReturn)
    }

    /// True where the document takes back one already posted, so every line moves the other way.
    fn reverses(self) -> bool {
        matches!(self, Self::SalesReturn | Self::PurchaseReturn)
    }
}

/// The same lines with every debit and credit swapped, which is what a return posts.
fn mirror(lines: Vec<Value>) -> Vec<Value> {
    lines
        .into_iter()
        .map(|line| {
            json!({
                "account_number": line["account_number"].clone(),
                "debit": line["credit"].clone(),
                "credit": line["debit"].clone(),
                "description": line["description"].clone(),
            })
        })
        .collect()
}

/// The taxes a document names, in the order it named them. Empty when the caller sent a tax figure instead.
fn named_taxes(payload: &Value) -> Vec<String> {
    payload
        .get("taxes")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(|one| one.trim().to_string())
                .filter(|one| !one.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The tax on a document whose figure is what the customer or supplier actually pays.
///
/// The tax comes out of that figure rather than being added on top, so a car sold for 100,000 has its tax taken out of
/// the 100,000. Each tax is returned with its own account, because each is owed to a different place and appears on a
/// different return, and one line per tax is what the journal posts.
fn tax_from_total(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    sale: bool,
    total: &Money,
) -> Result<(Money, Vec<TaxLine>), StoreError> {
    let names = named_taxes(payload);
    if names.is_empty() {
        return Ok((Money::zero(), Vec::new()));
    }
    let split = taxes::compute_tax(
        graph,
        scope,
        &json!({
            "date": text(payload, "entry_date"),
            "direction": if sale { "sale" } else { "purchase" },
            "gross": wire(total),
            "taxes": names,
        }),
    )?;
    let tax = Money::parse(field(&split, "tax"))
        .map_err(|_| refused("The tax on this document could not be read back."))?;
    let mut lines: Vec<TaxLine> = Vec::new();
    for line in split["tax_lines"].as_array().cloned().unwrap_or_default() {
        let amount = Money::parse(field(&line, "amount"))
            .map_err(|_| refused("A tax amount could not be read back."))?;
        if amount.is_zero() {
            continue;
        }
        lines.push((
            field(&line, "tax").to_string(),
            field(&line, "account_number").to_string(),
            field(&line, "expense_account").to_string(),
            amount,
        ));
    }
    Ok((tax, lines))
}

/// Post a taxed document with one line per tax, each to the account that tax names.
///
/// A single tax still goes through here, so the account posted to is the tax's own rather than whichever account the
/// Book happens to call VAT.
#[allow(clippy::too_many_arguments)]
fn post_taxed_lines(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    description: String,
    reference: &str,
    source_module: &str,
    kind: TaxedDocument,
    debit_account: &str,
    credit_account: &str,
    net: &Money,
    tax_lines: &[TaxLine],
    line_note: &str,
) -> Result<String, StoreError> {
    let zero = Money::zero();
    let mut total_tax = Money::zero();
    for (_, _, _, amount) in tax_lines {
        total_tax = total_tax.plus(amount);
    }
    let untaxed = net.minus(&total_tax);
    // a sale records the figure the customer paid and charges each tax to its own expense line, so the tax shows in
    // expenses by type and the sale is never understated; a purchase costs the net and reclaims each tax
    let sale = !kind.is_purchase();
    let mut lines: Vec<Value> = Vec::new();
    if sale {
        lines.push(json!({"account_number": debit_account, "debit": wire(net),
                          "credit": wire(&zero), "description": line_note}));
        lines.push(
            json!({"account_number": credit_account, "debit": wire(&zero),
                          "credit": wire(net), "description": line_note}),
        );
        for (name, account, expense, amount) in tax_lines {
            lines.push(json!({"account_number": expense, "debit": wire(amount),
                              "credit": wire(&zero),
                              "description": format!("{name} charged on {line_note}")}));
            lines.push(json!({"account_number": account, "debit": wire(&zero),
                              "credit": wire(amount),
                              "description": format!("{name} owed on {line_note}")}));
        }
    } else {
        lines.push(
            json!({"account_number": debit_account, "debit": wire(&untaxed),
                          "credit": wire(&zero), "description": line_note}),
        );
        for (name, account, _expense, amount) in tax_lines {
            lines.push(json!({"account_number": account, "debit": wire(amount),
                              "credit": wire(&zero),
                              "description": format!("{name} on {line_note}")}));
        }
        lines.push(
            json!({"account_number": credit_account, "debit": wire(&zero),
                          "credit": wire(net), "description": line_note}),
        );
    }
    let lines = if kind.reverses() {
        mirror(lines)
    } else {
        lines
    };
    let entry = json!({
        "entry_date": text(payload, "entry_date"),
        "description": description,
        "reference_number": reference,
        "source_module": source_module,
        "status": "posted",
        "created_at": text(payload, "created_at"),
        "lines": lines,
    });
    let posted = ledger_book::create_entry(graph, scope, &entry)?;
    Ok(field(&posted, "id").to_string())
}

#[allow(clippy::too_many_arguments)] // one argument per posting decision, mirroring the document
fn post_taxed(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    description: String,
    reference: &str,
    source_module: &str,
    kind: TaxedDocument,
    debit_account: &str,
    credit_account: &str,
    net: &Money,
    tax: &Money,
    line_note: &str,
) -> Result<String, StoreError> {
    let zero = Money::zero();
    let untaxed = net.minus(tax);
    let tax_account = tax_account(graph, scope, payload)?;
    // a sale records the figure the customer paid and charges the tax to its own expense account, so the tax shows in
    // expenses rather than being folded into the sale; a purchase costs net less tax and reclaims the tax
    let sale = !kind.is_purchase();
    let forward: Vec<Value> = if sale {
        let expense_account = taxes::tax_expense_account(graph, scope)?;
        vec![
            json!({"account_number": debit_account, "debit": wire(net), "credit": wire(&zero),
                   "description": line_note}),
            json!({"account_number": credit_account, "debit": wire(&zero), "credit": wire(net),
                   "description": line_note}),
            json!({"account_number": expense_account, "debit": wire(tax), "credit": wire(&zero),
                   "description": format!("Tax charged on {line_note}")}),
            json!({"account_number": tax_account, "debit": wire(&zero), "credit": wire(tax),
                   "description": format!("Tax owed on {line_note}")}),
        ]
    } else {
        vec![
            json!({"account_number": debit_account, "debit": wire(&untaxed), "credit": wire(&zero),
                   "description": line_note}),
            json!({"account_number": tax_account, "debit": wire(tax), "credit": wire(&zero),
                   "description": format!("Tax on {line_note}")}),
            json!({"account_number": credit_account, "debit": wire(&zero), "credit": wire(net),
                   "description": line_note}),
        ]
    };
    let lines = if kind.reverses() {
        mirror(forward)
    } else {
        forward
    };
    let entry = json!({
        "entry_date": text(payload, "entry_date"),
        "description": description,
        "reference_number": reference,
        "source_module": source_module,
        "status": "posted",
        "created_at": text(payload, "created_at"),
        "lines": lines,
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
    // a document raised in another money is carried into the Book's own before a single line is posted, so the ledger
    // never adds dollars to bond notes; the document keeps what it actually said
    let (converted, document) = crate::document_currency::in_book_money(graph, scope, payload)?;
    let payload = &converted;
    let _ = &document;
    let total = money(payload, "total_amount")?;
    let discount = money_or_nothing(payload, "discount_amount")?;
    let net = total.minus(&discount);
    let invoice = text(payload, "invoice_number");
    // a figure typed into the tax field is used as given; a document that names its taxes has them taken out of the total
    let typed_tax = money_or_nothing(payload, "tax_amount")?;
    let (tax, tax_lines) = if typed_tax.is_zero() {
        tax_from_total(graph, scope, payload, true, &net)?
    } else {
        (typed_tax, Vec::new())
    };
    check_tax_fits(&tax, &net, &format!("invoice {invoice}"))?;

    let entry_id = if tax.is_zero() {
        post(
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
        )?
    } else if !tax_lines.is_empty() {
        post_taxed_lines(
            graph,
            scope,
            payload,
            format!("Sales Journal: Invoice {invoice}"),
            &invoice,
            "SalesJournal",
            TaxedDocument::Sale,
            "1100",
            "4000",
            &net,
            &tax_lines,
            &format!("Sales Invoice {invoice}"),
        )?
    } else {
        post_taxed(
            graph,
            scope,
            payload,
            format!("Sales Journal: Invoice {invoice}"),
            &invoice,
            "SalesJournal",
            TaxedDocument::Sale,
            "1100",
            "4000",
            &net,
            &tax,
            &format!("Sales Invoice {invoice}"),
        )?
    };

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
            "document_currency": text(payload, "document_currency"),
            "document_rate": text(payload, "document_rate"),
            "document_total": text(payload, "document_total"),
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
    // a document raised in another money is carried into the Book's own before a single line is posted, so the ledger
    // never adds dollars to bond notes; the document keeps what it actually said
    let (converted, document) = crate::document_currency::in_book_money(graph, scope, payload)?;
    let payload = &converted;
    let _ = &document;
    let total = money(payload, "total_amount")?;
    let discount = money_or_nothing(payload, "discount_amount")?;
    let net = total.minus(&discount);
    let order = text(payload, "purchase_order_number");
    let typed_tax = money_or_nothing(payload, "tax_amount")?;
    let (tax, tax_lines) = if typed_tax.is_zero() {
        tax_from_total(graph, scope, payload, false, &net)?
    } else {
        (typed_tax, Vec::new())
    };
    check_tax_fits(&tax, &net, &format!("purchase {order}"))?;

    let entry_id = if tax.is_zero() {
        post(
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
        )?
    } else if !tax_lines.is_empty() {
        post_taxed_lines(
            graph,
            scope,
            payload,
            format!("Purchases Journal: PO {order}"),
            &order,
            "PurchasesJournal",
            TaxedDocument::Purchase,
            "1200",
            "2000",
            &net,
            &tax_lines,
            &format!("Purchase {order}"),
        )?
    } else {
        post_taxed(
            graph,
            scope,
            payload,
            format!("Purchases Journal: PO {order}"),
            &order,
            "PurchasesJournal",
            TaxedDocument::Purchase,
            "1200",
            "2000",
            &net,
            &tax,
            &format!("Purchase {order}"),
        )?
    };

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
            "document_currency": text(payload, "document_currency"),
            "document_rate": text(payload, "document_rate"),
            "document_total": text(payload, "document_total"),
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
    let reference = text(payload, "return_number");
    // what is credited back is what the customer paid, so the tax comes out of it exactly as it did on the sale
    let typed_tax = money_or_nothing(payload, "tax_amount")?;
    let (tax, tax_lines) = if typed_tax.is_zero() {
        tax_from_total(graph, scope, payload, true, &total)?
    } else {
        (typed_tax, Vec::new())
    };
    check_tax_fits(&tax, &total, &format!("sales return {reference}"))?;
    let net = total.minus(&tax);
    let note = format!("Sales Return {reference}");

    // a return is the sale turned round: the customer owes the total again, the sale is taken back in full and each tax
    // it carried leaves the expense it was charged to and the liability it was owed to
    let entry_id = if tax.is_zero() {
        post(
            graph,
            scope,
            payload,
            format!("Sales Returns: {reference}"),
            &reference,
            "SalesReturnsJournal",
            "4050",
            "1100",
            &net,
            &note,
        )?
    } else if !tax_lines.is_empty() {
        post_taxed_lines(
            graph,
            scope,
            payload,
            format!("Sales Returns: {reference}"),
            &reference,
            "SalesReturnsJournal",
            TaxedDocument::SalesReturn,
            "1100",
            "4050",
            &total,
            &tax_lines,
            &note,
        )?
    } else {
        post_taxed(
            graph,
            scope,
            payload,
            format!("Sales Returns: {reference}"),
            &reference,
            "SalesReturnsJournal",
            TaxedDocument::SalesReturn,
            "1100",
            "4050",
            &total,
            &tax,
            &note,
        )?
    };

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
    let reference = text(payload, "return_number");
    // what the supplier credits back is what was paid, so the tax comes out of it exactly as it did on the bill
    let typed_tax = money_or_nothing(payload, "tax_amount")?;
    let (tax, tax_lines) = if typed_tax.is_zero() {
        tax_from_total(graph, scope, payload, false, &total)?
    } else {
        (typed_tax, Vec::new())
    };
    check_tax_fits(&tax, &total, &format!("purchases return {reference}"))?;
    let net = total.minus(&tax);
    let note = format!("Purchases Return {reference}");

    // goods back to a supplier is the purchase turned round: the supplier is owed the total again, the cost is taken
    // back and each tax it carried leaves the reclaim it was charged to
    let entry_id = if tax.is_zero() {
        post(
            graph,
            scope,
            payload,
            format!("Purchases Returns: {reference}"),
            &reference,
            "PurchasesReturnsJournal",
            "2000",
            "5050",
            &net,
            &note,
        )?
    } else if !tax_lines.is_empty() {
        post_taxed_lines(
            graph,
            scope,
            payload,
            format!("Purchases Returns: {reference}"),
            &reference,
            "PurchasesReturnsJournal",
            TaxedDocument::PurchaseReturn,
            "5050",
            "2000",
            &total,
            &tax_lines,
            &note,
        )?
    } else {
        post_taxed(
            graph,
            scope,
            payload,
            format!("Purchases Returns: {reference}"),
            &reference,
            "PurchasesReturnsJournal",
            TaxedDocument::PurchaseReturn,
            "5050",
            "2000",
            &total,
            &tax,
            &note,
        )?
    };

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
    // a document raised in another money is carried into the Book's own before a single line is posted, so the ledger
    // never adds dollars to bond notes; the document keeps what it actually said
    let (converted, document) = crate::document_currency::in_book_money(graph, scope, payload)?;
    let payload = &converted;
    let _ = &document;
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
            "document_currency": text(payload, "document_currency"),
            "document_rate": text(payload, "document_rate"),
            "document_total": text(payload, "document_total"),
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
            ("4050", "revenue", "debit"),
            ("5050", "expense", "credit"),
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
    #[test]
    fn a_car_sold_for_a_hundred_thousand_has_the_tax_taken_out_of_the_total() {
        // the invoice says 100,000, so that is what the customer owes; the tax comes out of it, it is not added on
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        let scope = BookScope::new("user-1", "book-1");
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        taxes::save_tax(
            &graph,
            &scope,
            &json!({"name": "VAT", "rate": "15.5", "scope": "both",
                    "collected_account": "2100", "effective_from": "2026-01-01"}),
        )
        .expect("VAT defined");

        let sale = create_sale(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-19", "invoice_number": "CAR-1",
                    "customer_id": "c-1", "total_amount": "100000.00",
                    "taxes": ["VAT"]}),
        )
        .expect("posted");
        let entry_id = field(&sale, "journal_entry_id").to_string();
        let entry = ledger_book::get_entry(&graph, &scope, &entry_id).expect("the entry");
        let lines = entry["lines"].as_array().cloned().expect("lines");
        let debits: Money = lines.iter().fold(Money::zero(), |sum, line| {
            sum.plus(&money(line, "debit").expect("debit"))
        });
        let credits: Money = lines.iter().fold(Money::zero(), |sum, line| {
            sum.plus(&money(line, "credit").expect("credit"))
        });
        assert_eq!(debits, credits);
        // four lines: the customer owes the total, the sale records that same figure in full, and the tax it carries is
        // its own expense against its own payable account
        assert_eq!(lines.len(), 4);
        assert_eq!(field(&lines[0], "account_number"), "1100");
        assert_eq!(
            money(&lines[0], "debit").expect("owed"),
            Money::parse("100000.00").expect("total")
        );
        assert_eq!(field(&lines[1], "account_number"), "4000");
        assert_eq!(
            money(&lines[1], "credit").expect("sale"),
            Money::parse("100000.00").expect("the figure the customer paid"),
        );
        assert_eq!(field(&lines[2], "account_number"), "6900");
        assert_eq!(
            money(&lines[2], "debit").expect("tax expense"),
            Money::parse("13419.91").expect("tax")
        );
        assert_eq!(field(&lines[3], "account_number"), "2100");
        assert_eq!(
            money(&lines[3], "credit").expect("tax owed"),
            Money::parse("13419.91").expect("tax")
        );
        // and the document remembers the tax it carried
        assert_eq!(field(&sale, "tax_amount"), "13419.910000");
    }

    /// A Book with VAT defined, which a taxed document needs before it can be posted.
    fn book_with_vat() -> (vimbai_store::fake::MemoryGraph, BookScope) {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        let scope = BookScope::new("user-1", "book-1");
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        let reclaim = taxes::tax_expense_account(&graph, &scope).expect("a tax expense account");
        taxes::save_tax(
            &graph,
            &scope,
            &json!({"name": "VAT", "rate": "15.5", "scope": "both",
                    "collected_account": "2100", "reclaim_account": "2100",
                    "expense_account": reclaim, "effective_from": "2026-01-01"}),
        )
        .expect("VAT defined");
        (graph, scope)
    }

    fn lines_of(
        graph: &vimbai_store::fake::MemoryGraph,
        scope: &BookScope,
        id: &str,
    ) -> Vec<Value> {
        let entry = ledger_book::get_entry(graph, scope, id).expect("the entry");
        entry["lines"].as_array().cloned().expect("lines")
    }

    fn sum(lines: &[Value], side: &str) -> Money {
        lines.iter().fold(Money::zero(), |total, line| {
            total.plus(&money(line, side).expect(side))
        })
    }

    /// Goods come back: the sale is taken back in full, and the tax it carried leaves both the expense it was charged
    /// to and the liability it was owed to. What the customer owed is the figure they paid, not the net of it.
    #[test]
    fn a_sales_return_takes_the_sale_and_the_tax_back_out() {
        let (graph, scope) = book_with_vat();
        create_sale(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-19", "invoice_number": "CAR-1",
                    "customer_id": "c-1", "total_amount": "100000.00", "taxes": ["VAT"]}),
        )
        .expect("posted");

        let back = create_sales_return(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-20", "return_number": "CN-1",
                    "original_invoice_number": "CAR-1", "customer_id": "c-1",
                    "total_amount": "100000.00", "taxes": ["VAT"], "reason": "came back"}),
        )
        .expect("posted");
        let lines = lines_of(&graph, &scope, field(&back, "journal_entry_id"));
        assert_eq!(lines.len(), 4);
        assert_eq!(sum(&lines, "debit"), sum(&lines, "credit"));
        // the customer no longer owes it
        assert_eq!(field(&lines[0], "account_number"), "1100");
        assert_eq!(
            money(&lines[0], "credit").expect("owed"),
            total_of("100000.00")
        );
        // the sale is taken back in full, and the tax leaves the expense it was charged to
        assert_eq!(field(&lines[1], "account_number"), "4050");
        assert_eq!(
            money(&lines[1], "debit").expect("sale"),
            total_of("100000.00")
        );
        assert_eq!(field(&lines[2], "account_number"), "6900");
        assert_eq!(
            money(&lines[2], "credit").expect("tax"),
            total_of("13419.91")
        );
        // and it is no longer owed to the taxman
        assert_eq!(field(&lines[3], "account_number"), "2100");
        assert_eq!(
            money(&lines[3], "debit").expect("tax owed"),
            total_of("13419.91")
        );
        assert_eq!(field(&back, "tax_amount"), "13419.910000");
    }

    /// Goods go back to the supplier: the bill is taken back in full and the tax it carried leaves the reclaim.
    #[test]
    fn a_purchases_return_takes_the_bill_and_the_tax_back_out() {
        let (graph, scope) = book_with_vat();
        create_purchase(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-19", "purchase_order_number": "PO-1",
                    "vendor_id": "v-1", "total_amount": "460.00", "taxes": ["VAT"]}),
        )
        .expect("posted");

        let back = create_purchases_return(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-20", "return_number": "DN-1",
                    "original_po_number": "PO-1", "vendor_id": "v-1",
                    "total_amount": "460.00", "taxes": ["VAT"], "reason": "wrong item"}),
        )
        .expect("posted");
        let lines = lines_of(&graph, &scope, field(&back, "journal_entry_id"));
        assert_eq!(lines.len(), 3);
        assert_eq!(sum(&lines, "debit"), sum(&lines, "credit"));
        // the cost comes back off the account the bill put it on
        assert_eq!(field(&lines[0], "account_number"), "5050");
        assert_eq!(
            money(&lines[0], "credit").expect("cost"),
            total_of("398.27")
        );
        // the tax leaves the account it was reclaimed into, by crediting what the bill debited
        assert_eq!(field(&lines[1], "account_number"), "2100");
        assert_eq!(
            money(&lines[1], "credit").expect("reclaim"),
            total_of("61.73")
        );
        // and the supplier is no longer owed it
        assert_eq!(field(&lines[2], "account_number"), "2000");
        assert_eq!(money(&lines[2], "debit").expect("owed"), total_of("460.00"));
        assert_eq!(field(&back, "tax_amount"), "61.730000");
    }

    fn total_of(text: &str) -> Money {
        Money::parse(text).expect("an amount")
    }

    /// The chart these documents post to, with the one account that holds their tax.
    fn chart_with_tax() -> Vec<Value> {
        let mut rows = chart();
        rows.push(
            json!({"id": "a-2100", "account_number": "2100", "name": "VAT payable",
                         "account_type": "liability", "normal_balance": "credit"}),
        );
        rows.push(
            json!({"id": "a-6900", "account_number": "6900", "name": "Tax charged on sales",
                         "account_type": "expense", "normal_balance": "debit"}),
        );
        rows
    }

    /// A document that carries tax reads the chart for the expense account its tax is charged to, then once per line when
    /// the posting validates them (four lines for a taxed sale: the debt, the sale, the tax expense and the tax owed),
    /// then the months closed, then the chain, then the entry write and the document write.
    fn posted_entry_with_tax(
        reads: usize,
        build: impl FnOnce(&FakeGraph) -> Result<Value, StoreError>,
    ) -> Value {
        let mut scripts: Vec<Vec<Value>> = (0..reads).map(|_| chart_with_tax()).collect();
        scripts.push(vec![]);
        scripts.push(vec![]);
        scripts.push(vec![json!({"id": "je-1"})]);
        scripts.push(vec![json!({"id": "doc-1"})]);
        let graph = FakeGraph::queue(scripts);
        build(&graph).expect("created");
        let (statement, params) = graph.statement(reads + 2);
        assert!(statement.contains("CREATE"), "{statement}");
        let mut entry = params["props"].as_object().cloned().expect("an entry");
        entry.insert("lines".into(), params["children"].clone());
        Value::Object(entry)
    }

    fn posted_entry(build: impl FnOnce(&FakeGraph) -> Result<Value, StoreError>) -> Value {
        // two chart reads (one per line), then the months closed, then the chain, then the entry write, then the journal
        // write
        let graph = FakeGraph::queue(vec![
            chart(),
            chart(),
            vec![],
            vec![],
            vec![json!({"id": "je-1"})],
            vec![json!({"id": "doc-1"})],
        ]);
        build(&graph).expect("created");
        let (statement, params) = graph.statement(4);
        assert!(statement.contains("CREATE"), "{statement}");
        // the lines are written as their own records now, carried in the same statement as the entry
        let mut entry = params["props"].as_object().cloned().expect("an entry");
        entry.insert("lines".into(), params["children"].clone());
        Value::Object(entry)
    }

    #[test]
    fn a_sales_invoice_posts_the_total_less_discount_and_the_tax_as_its_own_line() {
        // the tax account, the expense account the tax is charged to, and four lines
        let entry = posted_entry_with_tax(6, |graph| {
            create_sale(
                graph,
                &scope(),
                &json!({"invoice_number": "INV-1", "customer_id": "cust-1",
                        "entry_date": "2026-03-15", "total_amount": "1000.00",
                        "discount_amount": "50.00", "tax_amount": "140.00"}),
            )
        });
        let lines = entry["lines"].as_array().expect("lines");
        // the customer owes the whole invoice
        assert_eq!(lines[0]["account_number"], json!("1100"));
        assert_eq!(lines[0]["debit"], json!("950.000000"));
        // the sale records the figure the customer paid, in full
        assert_eq!(lines[1]["account_number"], json!("4000"));
        assert_eq!(lines[1]["credit"], json!("950.000000"));
        // the tax it carries is its own expense, and is owed to somebody else on its own line
        assert_eq!(lines[2]["account_number"], json!("6900"));
        assert_eq!(lines[2]["debit"], json!("140.000000"));
        assert_eq!(lines[3]["account_number"], json!("2100"));
        assert_eq!(lines[3]["credit"], json!("140.000000"));
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn a_taxed_document_in_a_book_with_no_tax_account_is_refused_rather_than_posting_to_nothing() {
        // the chart this Book holds has no account whose name mentions VAT or tax
        let graph = FakeGraph::queue(vec![chart(), chart(), chart()]);
        let refused = create_sale(
            &graph,
            &scope(),
            &json!({"invoice_number": "INV-2", "entry_date": "2026-03-15",
                    "total_amount": "115.00", "tax_amount": "15.00"}),
        )
        .expect_err("no tax account");
        let words = refused.to_string();
        assert!(words.contains("no VAT account"), "{words}");
        assert!(words.contains("tax_account_number"), "{words}");
    }

    #[test]
    fn a_tax_larger_than_the_document_is_refused_naming_both_figures() {
        let graph = FakeGraph::returning(chart());
        let refused = create_sale(
            &graph,
            &scope(),
            &json!({"invoice_number": "INV-3", "entry_date": "2026-03-15",
                    "total_amount": "100.00", "tax_amount": "150.00"}),
        )
        .expect_err("tax above the total");
        let words = refused.to_string();
        assert!(words.contains("150.00"), "{words}");
        assert!(words.contains("100.00"), "{words}");
        assert!(words.contains("which figure is the tax"), "{words}");
    }

    #[test]
    fn a_purchase_with_tax_reclaims_it_rather_than_costing_the_business_for_it() {
        // the tax account and three lines: a purchase reclaims its tax, it does not charge one
        let entry = posted_entry_with_tax(4, |graph| {
            create_purchase(
                graph,
                &scope(),
                &json!({"purchase_order_number": "PO-1", "vendor_id": "v-1",
                        "entry_date": "2026-03-15", "total_amount": "460.00",
                        "tax_amount": "60.00"}),
            )
        });
        let lines = entry["lines"].as_array().expect("lines");
        assert_eq!(lines[0]["account_number"], json!("1200"));
        assert_eq!(lines[0]["debit"], json!("400.000000"));
        assert_eq!(lines[1]["account_number"], json!("2100"));
        assert_eq!(lines[1]["debit"], json!("60.000000"));
        assert_eq!(lines[2]["account_number"], json!("2000"));
        assert_eq!(lines[2]["credit"], json!("460.000000"));
    }

    #[test]
    fn a_purchase_return_reduces_cost_rather_than_raising_it() {
        let entry = posted_entry(|graph| {
            create_purchases_return(
                graph,
                &scope(),
                &json!({"return_number": "PR-1", "vendor_id": "v-1", "entry_date": "2026-03-15",
                        "total_amount": "100.00"}),
            )
        });
        let lines = entry["lines"].as_array().expect("lines");
        // the supplier is no longer owed it, and the cost comes back off the returns account; the shipped service
        // debited the expense account, which raised the cost of goods a return had just taken back
        assert_eq!(lines[0]["account_number"], json!("2000"));
        assert_eq!(lines[0]["debit"], json!("100.000000"));
        assert_eq!(lines[1]["account_number"], json!("5050"));
        assert_eq!(lines[1]["credit"], json!("100.000000"));
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
