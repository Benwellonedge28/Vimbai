//! The discounts a Book gives.
//!
//! A discount is either a percentage or a sum of money, and it says who it is for: every customer, or only the ones it
//! names. It also says whether it is compulsory - given on every invoice it fits, without anybody asking - or offered,
//! so a person raising an invoice may take it or leave it.
//!
//! A rule is checked here rather than in a screen, so every route to a discount says the same thing. It is dated like a
//! tax, because a promotion ends and the invoices raised under it must keep the figure they were raised with.
//!
//! The arithmetic is exact and works to the cent: a percentage is taken off what is left at that point in the order the
//! rules were declared, an amount is taken off as it stands, and no rule may take more than the invoice comes to. What
//! comes off is what the customer does not pay, so it is also what they are not taxed on.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, wire};
use vimbai_store::read::{refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

use crate::invoices::{customer_known, INVOICE_DATE_KEY};

/// A discount the Book gives, kept as its own record so a rule can be read, dated and retired without touching an
/// invoice that was raised under it.
pub const DISCOUNT_RULE: RecordKind = RecordKind::new("DiscountRule", "OWNS_DISCOUNT_RULE");

/// How much a rule takes off, and by which method.
const PERCENT: &str = "percent";
const AMOUNT: &str = "amount";

/// What one rule took off one invoice, so the invoice can say where its discount came from.
#[derive(Clone, Debug)]
pub struct Applied {
    /// One line per rule that applied, naming it and what it took off.
    pub lines: Vec<Value>,
    /// Everything that came off the invoice together.
    pub total: Money,
}

impl Applied {
    fn nothing() -> Self {
        Applied {
            lines: Vec::new(),
            total: Money::zero(),
        }
    }
}

/// A rule as it was declared.
#[derive(Clone, Debug)]
struct Rule {
    id: String,
    name: String,
    kind: String,
    value: Money,
    compulsory: bool,
    all_customers: bool,
    customer_ids: Vec<String>,
    min_total: Money,
    from: String,
    until: String,
}

impl Rule {
    fn from_record(record: &Value) -> Option<Rule> {
        let name = text(record, "name");
        if name.is_empty() {
            return None;
        }
        let kind = match text(record, "kind").as_str() {
            PERCENT => PERCENT,
            _ => AMOUNT,
        };
        Some(Rule {
            id: text(record, "id"),
            name,
            kind: kind.to_string(),
            value: amount_or_zero(record, "value").ok()?,
            compulsory: record
                .get("compulsory")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            all_customers: record
                .get("all_customers")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            customer_ids: record
                .get("customer_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(|one| one.trim().to_string())
                        .filter(|one| !one.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            min_total: amount_or_zero(record, "min_total").unwrap_or_else(|_| Money::zero()),
            from: text(record, "effective_from"),
            until: text(record, "effective_to"),
        })
    }

    /// Whether this rule is meant for this customer on this invoice.
    fn fits(&self, total: &Money, customer_id: &str, date: &str) -> bool {
        if !self.from.is_empty() && !date.is_empty() && date < self.from.as_str() {
            return false;
        }
        if !self.until.is_empty() && !date.is_empty() && date > self.until.as_str() {
            return false;
        }
        if total < &self.min_total {
            return false;
        }
        if self.all_customers {
            return true;
        }
        self.customer_ids.iter().any(|one| one == customer_id)
    }

    /// What this rule takes off the figure it is given.
    fn takes_off(&self, running: &Money) -> Result<Money, StoreError> {
        let off = if self.kind == PERCENT {
            running
                .times(&self.value)
                .over(&Money::from_i64(100))
                .round(2)
        } else {
            self.value.clone()
        };
        if off.is_negative() {
            return Err(refused(&format!(
                "{} takes off less than nothing, which is a surcharge rather than a discount.",
                self.name
            )));
        }
        if &off > running || (off == *running && !off.is_zero()) {
            return Err(refused(&format!(
                "{} would take {} off an invoice of {}, leaving nothing to bill. A discount leaves something to pay.",
                self.name,
                off.to_fixed(2),
                running.to_fixed(2)
            )));
        }
        Ok(off)
    }
}

/// `POST /invoices/discounts`: declare a discount this Book gives.
pub fn save_discount<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name").trim().to_string();
    if name.is_empty() {
        return Err(refused(
            "Give the discount a name, like Early settlement or Trade terms.",
        ));
    }

    // a discount is a percentage or a sum of money, and which one it is decides how its value reads
    let kind = match text(payload, "kind").to_lowercase().as_str() {
        "" | "amount" | "money" | "fixed" => AMOUNT,
        "percent" | "percentage" | "%" => PERCENT,
        other => {
            return Err(refused(&format!(
                "A discount is a percentage or a sum of money, not {other}. Say percent or amount."
            )))
        }
    };
    let value = amount_or_zero(payload, "value")?;
    if value.is_negative() || value.is_zero() {
        return Err(refused(&format!(
            "{name} has to take something off. Give the {} it takes off.",
            if kind == PERCENT {
                "percentage"
            } else {
                "amount"
            }
        )));
    }
    if kind == PERCENT && value > Money::from_i64(100) {
        return Err(refused(&format!(
            "{name} cannot be more than 100 per cent, or it would take the whole invoice."
        )));
    }

    let from = text(payload, "effective_from");
    if !is_day(&from) {
        return Err(refused(&format!(
            "Say the day {name} starts to apply, as a date like 2026-01-01."
        )));
    }
    // a discount is a thing that runs its course: it says when it starts and when it stops, so a rule cannot sit in
    // the Book taking money off invoices nobody meant it to reach
    let until = text(payload, "effective_to");
    if until.is_empty() {
        return Err(refused(&format!(
            "Say the day {name} stops applying, as a date like 2026-12-31. Every discount runs from a day to a day."
        )));
    }
    if !is_day(&until) {
        return Err(refused(
            "The day a discount stops applying reads like 2026-12-31.",
        ));
    }
    if until < from {
        return Err(refused(&format!(
            "{name} would stop applying on {until}, before it starts on {from}."
        )));
    }

    // who it is for: everyone, or the customers named
    let all_customers = payload
        .get("all_customers")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let customer_ids: Vec<String> = payload
        .get("customer_ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(|one| one.trim().to_string())
                .filter(|one| !one.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !all_customers && customer_ids.is_empty() {
        return Err(refused(&format!(
            "{name} is not for everyone, so name the customers it is for, or say it is for all customers."
        )));
    }
    for wanted in &customer_ids {
        if !customer_known(graph, scope, wanted)? {
            return Err(refused(&format!(
                "{name} is for customer {wanted}, but this Book has no customer with that identifier. Add the customer \
                 first, or give the discount to all customers."
            )));
        }
    }

    let min_total = amount_or_zero(payload, "min_total")?;
    if min_total.is_negative() {
        return Err(refused(&format!(
            "{name} cannot start at a figure below nothing. Leave the starting figure empty for every invoice."
        )));
    }

    let compulsory = payload
        .get("compulsory")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // one discount, one definition at a time: two rules of the same name cannot both hold on the same day, or nobody
    // could say which was given
    let id = text(payload, "id");
    let existing = all_rules(graph, scope)?;
    for rule in &existing {
        if rule.name != name || rule.id == id {
            continue;
        }
        let overlaps = rule.from.as_str() <= until_or_forever(&until).as_str()
            && (rule.until.is_empty() || from.as_str() <= rule.until.as_str());
        if overlaps {
            return Err(refused(&format!(
                "This Book already has a discount called {name} that holds on those days. Change its value instead of \
                 adding a second one, or give this one a different name."
            )));
        }
    }

    let record = json!({
        "id": if id.is_empty() { service_id() } else { id },
        "name": name,
        "kind": kind,
        "value": wire(&value),
        "compulsory": compulsory,
        "all_customers": all_customers,
        "customer_ids": customer_ids,
        "min_total": wire(&min_total),
        "effective_from": from,
        "effective_to": until,
        "note": text(payload, "note"),
        "created_at": stamp(payload),
        "updated_at": stamp(payload),
    });
    let saved = match record["id"].as_str() {
        Some(existing_id)
            if !existing_id.is_empty() && existing.iter().any(|r| r.id == existing_id) =>
        {
            Records::new(graph, DISCOUNT_RULE).update(scope, existing_id, &record)?
        }
        _ => Records::new(graph, DISCOUNT_RULE).create(scope, &record)?,
    };
    Ok(saved)
}

/// `GET /invoices/discounts`: the discounts this Book gives, retired ones included so their history is readable.
pub fn discount_rules<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, DISCOUNT_RULE).list(scope, 500)?,
    ))
}

/// `POST /invoices/discounts/quote`: what would come off a given invoice, without raising one.
pub fn quote_discounts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = amount_or_zero(payload, "total")?;
    let customer_id = text(payload, "customer_id");
    let date = text(payload, "date");
    let asked = asked_rules(payload);
    let applied = apply_for(graph, scope, &total, &customer_id, &date, &asked)?;
    Ok(json!({
        "total": wire(&total),
        "discount": wire(&applied.total),
        "after": wire(&total.minus(&applied.total)),
        "lines": applied.lines,
    }))
}

/// Everything that comes off this invoice: the compulsory rules that fit it, the offered ones the caller named, in the
/// order the Book declared them.
pub fn apply_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    total: &Money,
    customer_id: &str,
    date: &str,
    asked: &[String],
) -> Result<Applied, StoreError> {
    let mut applied = Applied::nothing();
    let mut running = total.clone();
    for rule in all_rules(graph, scope)? {
        let fits = rule.fits(&running, customer_id, date);
        let taken = rule.compulsory || asked.iter().any(|one| one == &rule.name);
        if !fits || !taken {
            continue;
        }
        let off = rule.takes_off(&running)?;
        if off.is_zero() {
            continue;
        }
        running = running.minus(&off);
        applied.total = applied.total.plus(&off);
        applied.lines.push(json!({
            "name": rule.name,
            "kind": rule.kind,
            "value": wire(&rule.value),
            "amount": wire(&off),
        }));
    }
    Ok(applied)
}

/// The rule names a caller asked to take. A discount that is not compulsory is never assumed.
pub fn asked_rules(payload: &Value) -> Vec<String> {
    payload
        .get("discount_rules")
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

/// Every discount the Book has defined, retired ones included, so a person can see their history.
fn all_rules<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Vec<Rule>, StoreError> {
    let records = Records::new(graph, DISCOUNT_RULE).list(scope, 500)?;
    Ok(records.iter().filter_map(Rule::from_record).collect())
}

fn until_or_forever(until: &str) -> String {
    if until.is_empty() {
        "9999-12-31".to_string()
    } else {
        until.to_string()
    }
}

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

/// A date written as a day, which is what a rule is dated by.
fn is_day(date: &str) -> bool {
    let date = date.trim();
    if date.len() != 10 || date.as_bytes()[4] != b'-' || date.as_bytes()[7] != b'-' {
        return false;
    }
    let month: u32 = match date[5..7].parse() {
        Ok(month) => month,
        Err(_) => return false,
    };
    let day: u32 = match date[8..10].parse() {
        Ok(day) => day,
        Err(_) => return false,
    };
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// The date an invoice is dated by, which is what decides the rules that were in force when it was raised.
pub fn rule_date(payload: &Value) -> String {
    let stated = text(payload, INVOICE_DATE_KEY);
    if is_day(&stated) {
        return stated;
    }
    text(payload, "date")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoices::create_invoice_customer;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::read::field;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn customer(graph: &MemoryGraph, id: &str) {
        create_invoice_customer(
            graph,
            &talked(),
            &json!({"customer_id": id, "name": id, "now": 1000.0}),
        )
        .expect("a customer");
    }

    fn rule(graph: &MemoryGraph, fields: Value) -> Value {
        save_discount(graph, &talked(), &fields).expect("a discount")
    }

    fn percent_all(graph: &MemoryGraph, name: &str, value: &str) {
        rule(
            graph,
            json!({"name": name, "kind": "percent", "value": value, "compulsory": true,
                   "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        );
    }

    #[test]
    fn a_percentage_for_every_customer_comes_off_every_invoice_without_being_asked() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        percent_all(&graph, "Trade terms", "10");
        let applied = apply_for(
            &graph,
            &talked(),
            &Money::parse("1000.00").expect("total"),
            "ACME-1",
            "2026-11-19",
            &[],
        )
        .expect("applied");
        assert_eq!(applied.total.to_fixed(2), "100.00");
        assert_eq!(field(&applied.lines[0], "name"), "Trade terms");
        assert_eq!(field(&applied.lines[0], "amount"), "100.000000");
    }

    #[test]
    fn a_sum_of_money_can_be_given_to_the_customers_named_and_no_one_else() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        customer(&graph, "OTHER-2");
        rule(
            &graph,
            json!({"name": "Loyalty", "kind": "amount", "value": "250.00", "compulsory": true,
                   "all_customers": false, "customer_ids": ["ACME-1"], "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        );
        let total = Money::parse("1000.00").expect("total");
        let for_acme =
            apply_for(&graph, &talked(), &total, "ACME-1", "2026-11-19", &[]).expect("applied");
        assert_eq!(for_acme.total.to_fixed(2), "250.00");
        let for_other =
            apply_for(&graph, &talked(), &total, "OTHER-2", "2026-11-19", &[]).expect("applied");
        assert_eq!(
            for_other.total.to_fixed(2),
            "0.00",
            "a named discount is not for everyone"
        );
        assert!(for_other.lines.is_empty());
    }

    #[test]
    fn a_rule_can_wait_until_an_invoice_is_worth_it() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        rule(
            &graph,
            json!({"name": "Bulk", "kind": "percent", "value": "5", "compulsory": true,
                   "min_total": "5000.00", "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        );
        let small = apply_for(
            &graph,
            &talked(),
            &Money::parse("4000.00").expect("total"),
            "ACME-1",
            "2026-11-19",
            &[],
        )
        .expect("applied");
        assert_eq!(small.total.to_fixed(2), "0.00");
        let big = apply_for(
            &graph,
            &talked(),
            &Money::parse("5000.00").expect("total"),
            "ACME-1",
            "2026-11-19",
            &[],
        )
        .expect("applied");
        assert_eq!(big.total.to_fixed(2), "250.00");
    }

    #[test]
    fn an_offered_discount_is_never_assumed_and_comes_off_when_it_is_named() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        rule(
            &graph,
            json!({"name": "Early settlement", "kind": "percent", "value": "2",
                   "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        );
        let total = Money::parse("1000.00").expect("total");
        let unbidden =
            apply_for(&graph, &talked(), &total, "ACME-1", "2026-11-19", &[]).expect("applied");
        assert_eq!(unbidden.total.to_fixed(2), "0.00");
        let named = apply_for(
            &graph,
            &talked(),
            &total,
            "ACME-1",
            "2026-11-19",
            &["Early settlement".to_string()],
        )
        .expect("applied");
        assert_eq!(named.total.to_fixed(2), "20.00");
    }

    #[test]
    fn two_discounts_take_off_in_the_order_they_were_declared() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        percent_all(&graph, "Trade terms", "10");
        rule(
            &graph,
            json!({"name": "Delivery", "kind": "amount", "value": "100.00", "compulsory": true,
                   "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        );
        let applied = apply_for(
            &graph,
            &talked(),
            &Money::parse("1000.00").expect("total"),
            "ACME-1",
            "2026-11-19",
            &[],
        )
        .expect("applied");
        // 10% of 1,000 is 100, then a further 100 comes off, so 800 is left to pay
        assert_eq!(applied.total.to_fixed(2), "200.00");
        assert_eq!(applied.lines.len(), 2);
    }

    #[test]
    fn a_rule_that_would_take_the_whole_invoice_is_refused_rather_than_leaving_nothing() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        rule(
            &graph,
            json!({"name": "Grand opening", "kind": "amount", "value": "1000.00", "compulsory": true,
                   "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        );
        let refused = apply_for(
            &graph,
            &talked(),
            &Money::parse("1000.00").expect("total"),
            "ACME-1",
            "2026-11-19",
            &[],
        )
        .expect_err("nothing left to bill");
        assert!(
            refused.to_string().contains("leaving nothing to bill"),
            "{refused}"
        );
    }

    #[test]
    fn a_discount_is_refused_when_its_value_kind_or_customers_make_no_sense() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        let no_name = save_discount(
            &graph,
            &talked(),
            &json!({"kind": "percent", "value": "5",
            "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        )
        .expect_err("no name");
        assert!(
            no_name.to_string().contains("Give the discount a name"),
            "{no_name}"
        );

        let bad_kind = save_discount(
            &graph,
            &talked(),
            &json!({"name": "Odd", "kind": "voucher",
            "value": "5", "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        )
        .expect_err("bad kind");
        assert!(
            bad_kind.to_string().contains("percent or amount"),
            "{bad_kind}"
        );

        let too_much = save_discount(
            &graph,
            &talked(),
            &json!({"name": "Everything", "kind": "percent",
            "value": "120", "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        )
        .expect_err("over 100");
        assert!(
            too_much.to_string().contains("more than 100 per cent"),
            "{too_much}"
        );

        let nobody = save_discount(
            &graph,
            &talked(),
            &json!({"name": "Someone", "kind": "amount",
            "value": "10", "all_customers": false, "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        )
        .expect_err("no customers");
        assert!(
            nobody.to_string().contains("name the customers"),
            "{nobody}"
        );

        let stranger = save_discount(
            &graph,
            &talked(),
            &json!({"name": "Stranger", "kind": "amount",
            "value": "10", "all_customers": false, "customer_ids": ["NOBODY-9"],
            "effective_from": "2026-01-01", "effective_to": "2027-12-31"}),
        )
        .expect_err("unknown customer");
        assert!(
            stranger
                .to_string()
                .contains("no customer with that identifier"),
            "{stranger}"
        );
    }

    #[test]
    fn a_discount_stops_applying_after_the_day_it_was_given_until() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        rule(
            &graph,
            json!({"name": "November only", "kind": "percent", "value": "10", "compulsory": true,
                   "effective_from": "2026-11-01", "effective_to": "2026-11-30"}),
        );
        let total = Money::parse("1000.00").expect("total");
        let inside =
            apply_for(&graph, &talked(), &total, "ACME-1", "2026-11-19", &[]).expect("applied");
        assert_eq!(inside.total.to_fixed(2), "100.00");
        let after =
            apply_for(&graph, &talked(), &total, "ACME-1", "2026-12-01", &[]).expect("applied");
        assert_eq!(after.total.to_fixed(2), "0.00");
    }

    #[test]
    fn the_quote_reads_what_would_come_off_without_raising_an_invoice() {
        let graph = MemoryGraph::new();
        customer(&graph, "ACME-1");
        percent_all(&graph, "Trade terms", "10");
        let quote = quote_discounts(
            &graph,
            &talked(),
            &json!({"total": "1000.00", "customer_id": "ACME-1", "date": "2026-11-19"}),
        )
        .expect("quoted");
        assert_eq!(field(&quote, "discount"), "100.000000");
        assert_eq!(field(&quote, "after"), "900.000000");
        assert_eq!(quote["lines"].as_array().expect("lines").len(), 1);
    }

    #[test]
    fn a_discount_that_never_stops_is_refused_because_it_would_outlive_its_reason() {
        let graph = MemoryGraph::new();
        let answer = save_discount(
            &graph,
            &talked(),
            &json!({"name": "Forever", "kind": "percent", "value": "5", "compulsory": true,
                    "effective_from": "2026-01-01"}),
        );
        let refusal = answer.expect_err("a discount with no end date");
        assert!(
            format!("{refusal}").contains("stops applying"),
            "the refusal names the missing end: {refusal}"
        );

        let held = save_discount(
            &graph,
            &talked(),
            &json!({"name": "Forever", "kind": "percent", "value": "5", "compulsory": true,
                    "effective_from": "2026-01-01", "effective_to": "2026-12-31"}),
        )
        .expect("a discount from a day to a day");
        assert_eq!(held["effective_to"], json!("2026-12-31"));
    }
}
