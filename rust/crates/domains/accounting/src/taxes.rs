#![forbid(unsafe_code)]
//! The taxes a Book charges, defined by the Book rather than by this app.
//!
//! A tax here is a rule, not a number in the code: a name, the exact percentage it is charged at, the account its money
//! goes to, whether it applies to sales, to purchases or to both, the dates it holds for, and which taxes it applies
//! after. Everything that computes tax reads this table, so a new tax type is a row the owner adds, never a release of
//! this app. That is the whole point of the shape: PAYE, a sector levy or a second VAT rate all arrive as data.
//!
//! Two things are deliberate and worth keeping:
//!
//! * a rate is dated. A change is a new row with a later `effective_from`, never an edit of the old one, so a document
//!   posted last year keeps the rate it was posted under and history is corrected by reversal, not by rewriting it.
//! * order is declared per tax, through `applies_after`. VAT applies to the net; a levy that the law charges on the
//!   VAT-inclusive figure names VAT as what it applies after. One global sequence would be wrong for at least one tax.
//!
//! Nothing is posted here. This module answers what the tax on a document would be, and which account each tax lands in,
//! so the posting path and the phone can show the same figure before anything is written.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, TAX_RATE};

use crate::ledger_book;
use crate::periods::month_of;

/// Tax amounts are money, so they are rounded to the cent, the same scale the ledger keeps.
const SCALE: i64 = 2;

/// One tax as the Book defines it.
#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub name: String,
    /// The percentage, exactly: 15.5 is fifteen and a half per cent, never a float's approximation of it.
    pub rate: Money,
    pub applies_to_sales: bool,
    pub applies_to_purchases: bool,
    /// Where tax charged on a sale goes. Empty when this tax is not charged on sales.
    pub collected_account: String,
    /// Where tax paid on a purchase is reclaimed. Empty when this tax is not paid on purchases.
    pub reclaim_account: String,
    /// The taxes whose amount is added to this one's base before it is charged.
    pub after: Vec<String>,
    pub from: String,
    /// Empty when the tax still holds.
    pub until: String,
}

impl Rule {
    fn from_record(record: &Value) -> Option<Rule> {
        let name = field(record, "name").trim().to_string();
        if name.is_empty() {
            return None;
        }
        let rate = Money::parse(field(record, "rate")).ok()?;
        let scope = field(record, "scope").to_lowercase();
        Some(Rule {
            id: field(record, "id").to_string(),
            name,
            rate,
            applies_to_sales: scope != "purchases",
            applies_to_purchases: scope != "sales",
            collected_account: field(record, "collected_account").to_string(),
            reclaim_account: field(record, "reclaim_account").to_string(),
            after: record
                .get("applies_after")
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|one| one.trim().to_string())
                        .filter(|one| !one.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            from: field(record, "effective_from").to_string(),
            until: field(record, "effective_to").to_string(),
        })
    }

    /// Whether this definition is the one in force on a date. A tax with no end date still holds.
    pub fn in_force(&self, on: &str) -> bool {
        self.from.as_str() <= on && (self.until.is_empty() || on <= self.until.as_str())
    }

    /// Whether this tax is charged on a sale (or on a purchase).
    pub fn applies(&self, sale: bool) -> bool {
        if sale {
            self.applies_to_sales
        } else {
            self.applies_to_purchases
        }
    }

    /// The account this tax lands in for a direction, or empty when the Book has not said.
    pub fn account(&self, sale: bool) -> &str {
        if sale {
            &self.collected_account
        } else {
            &self.reclaim_account
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "rate": self.rate.to_fixed(4),
            "scope": if self.applies_to_sales && self.applies_to_purchases {
                "both"
            } else if self.applies_to_sales {
                "sales"
            } else {
                "purchases"
            },
            "collected_account": self.collected_account,
            "reclaim_account": self.reclaim_account,
            "applies_after": self.after,
            "effective_from": self.from,
            "effective_to": self.until,
        })
    }
}

/// Whether a string is a day: `2026-01-01`, not a month and not a year.
///
/// The month check is the one the closed-period rule already uses, so a date is read one way in this Book; the extra step
/// here is the day, because a rate starts on a day and a year alone cannot say which.
fn is_day(date: &str) -> bool {
    let date = date.trim();
    if date.len() != 10 || month_of(date).is_none() {
        return false;
    }
    let day = &date[8..];
    day.chars().all(|digit| digit.is_ascii_digit())
        && day.parse::<u32>().map(|day| (1..=31).contains(&day)) == Ok(true)
}

/// Every tax the Book has defined, retired ones included, so a person can see their history.
fn all_rules<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Vec<Rule>, StoreError> {
    let records = Records::new(graph, TAX_RATE).list(scope, 500)?;
    Ok(records.iter().filter_map(Rule::from_record).collect())
}

/// The taxes in force on a date, in the order they must be charged.
///
/// The order comes from each tax's own `applies_after`: a tax is charged after the taxes it names, so a levy on the
/// VAT-inclusive figure follows VAT. Taxes with nothing declared come first, in the order they were defined.
fn rules_in_force<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    on: &str,
    sale: bool,
) -> Result<Vec<Rule>, StoreError> {
    let in_force: Vec<Rule> = all_rules(graph, scope)?
        .into_iter()
        .filter(|rule| rule.in_force(on) && rule.applies(sale))
        .collect();
    Ok(ordered(in_force))
}

/// Sort taxes so each one follows everything it names, refusing rather than guessing if the chain loops.
fn ordered(rules: Vec<Rule>) -> Vec<Rule> {
    let names: Vec<String> = rules.iter().map(|rule| rule.name.clone()).collect();
    let mut placed: Vec<Rule> = Vec::new();
    let mut visiting: Vec<String> = Vec::new();
    let mut done: Vec<String> = Vec::new();

    fn visit(
        name: &str,
        rules: &[Rule],
        names: &[String],
        placed: &mut Vec<Rule>,
        visiting: &mut Vec<String>,
        done: &mut Vec<String>,
    ) {
        if done.iter().any(|held| held == name) || visiting.iter().any(|held| held == name) {
            // already placed, or on the stack now; the save path refuses a loop, so this only ends a repeated visit
            return;
        }
        let Some(rule) = rules.iter().find(|rule| rule.name == name) else {
            return;
        };
        visiting.push(name.to_string());
        for earlier in &rule.after {
            if names.iter().any(|held| held == earlier) {
                visit(earlier, rules, names, placed, visiting, done);
            }
        }
        visiting.pop();
        done.push(name.to_string());
        placed.push(rule.clone());
    }

    for name in &names {
        visit(name, &rules, &names, &mut placed, &mut visiting, &mut done);
    }
    placed
}

/// `GET /accounting/taxes/`: every tax the Book has defined, and which of them hold today.
pub fn list_taxes<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let on = text(query, "date");
    let today = if on.is_empty() { String::new() } else { on };
    if !today.is_empty() && !is_day(&today) {
        return Err(refused(
            "A date reads like 2026-11-19, so the taxes in force can be told apart from the retired ones.",
        ));
    }
    let rules = all_rules(graph, scope)?;
    let rows: Vec<Value> = rules
        .iter()
        .map(|rule| {
            let mut row = rule.to_json();
            if !today.is_empty() {
                row["in_force"] = json!(rule.in_force(&today));
            }
            row
        })
        .collect();
    Ok(json!({
        "taxes": rows,
        "count": rules.len(),
        "note": if rules.is_empty() {
            "This Book has not defined any tax yet. Add one, or seed the usual ones and confirm the rates."
        } else {
            "Each tax posts to its own account, so what is owed is never one merged figure."
        },
    }))
}

/// `POST /accounting/taxes/`: define a tax, or change one that has not been used yet.
///
/// A change that would rewrite history is refused: a rate that has already been posted is retired with an end date and
/// the new rate starts on a day of its own. That is the difference between a rate table and a guess about the past.
pub fn save_tax<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name").trim().to_string();
    if name.is_empty() {
        return Err(refused(
            "Give the tax a name, like VAT or Withholding on contracts.",
        ));
    }
    let rate = match rate_of(payload)? {
        Some(rate) => rate,
        None => {
            return Err(refused(&format!(
                "Say the percentage {name} is charged at, like 15.5."
            )))
        }
    };
    if rate.is_negative() {
        return Err(refused(&format!(
            "{name} cannot be charged at less than nothing. Give the percentage, like 15.5."
        )));
    }
    if rate > Money::from_i64(100) {
        return Err(refused(&format!(
            "{name} cannot be charged at more than 100 per cent."
        )));
    }
    let scope_word = {
        let given = text(payload, "scope").to_lowercase();
        match given.as_str() {
            "" | "both" => "both",
            "sales" | "sale" => "sales",
            "purchases" | "purchase" => "purchases",
            other => {
                return Err(refused(&format!(
                    "A tax applies to sales, to purchases or to both, not to {other}."
                )))
            }
        }
    };
    let from = text(payload, "effective_from");
    if !is_day(&from) {
        return Err(refused(&format!(
            "Say the day {name} starts to apply, as a date like 2026-01-01."
        )));
    }
    let until = text(payload, "effective_to");
    if !until.is_empty() && !is_day(&until) {
        return Err(refused(
            "The day a tax stops applying reads like 2026-12-31. Leave it empty while it still holds.",
        ));
    }
    if !until.is_empty() && until < from {
        return Err(refused(&format!(
            "{name} would stop applying on {until}, before it starts on {from}."
        )));
    }
    let collected = text(payload, "collected_account").trim().to_string();
    let reclaim = text(payload, "reclaim_account").trim().to_string();
    if !rate.is_zero() && collected.is_empty() && reclaim.is_empty() {
        return Err(refused(&format!(
            "Say which account {name} posts to, or it cannot be charged. Add one in Chart of accounts."
        )));
    }
    let chart = ledger_book::accounts(graph, scope)?;
    for number in [&collected, &reclaim] {
        if number.is_empty() {
            continue;
        }
        if !chart
            .iter()
            .any(|account| field(account, "account_number") == number.as_str())
        {
            return Err(refused(&format!(
                "This Book's chart has no account {number}, so {name} cannot post there."
            )));
        }
    }
    let after: Vec<String> = payload
        .get("applies_after")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(|one| one.trim().to_string())
                .filter(|one| !one.is_empty() && *one != name)
                .collect()
        })
        .unwrap_or_default();

    let existing = all_rules(graph, scope)?;
    for earlier in &after {
        if !existing.iter().any(|rule| &rule.name == earlier) {
            return Err(refused(&format!(
                "{name} cannot apply after {earlier}, because this Book has no tax called {earlier}."
            )));
        }
    }
    // a loop would make the order unknowable, and an order is what compounding depends on
    for earlier in &after {
        if chain_reaches(&existing, earlier, &name, 0) {
            return Err(refused(&format!(
                "{name} cannot apply after {earlier}, because {earlier} already applies after {name}. \
                 Pick one order for the two of them."
            )));
        }
    }
    // one tax, one rate at a time: two definitions of the same name cannot both hold on the same day
    let id = text(payload, "id");
    for rule in &existing {
        if rule.name != name || rule.id == id {
            continue;
        }
        let overlaps = rule.from.as_str() <= until_or_forever(&until)
            && (rule.until.is_empty() || from.as_str() <= rule.until.as_str());
        if overlaps {
            return Err(refused(&format!(
                "{} already has a rate from {} ({}%). Give the new rate a day after {}, or end the old one first.",
                name,
                rule.from,
                rule.rate.to_fixed(4),
                rule.until
            )));
        }
    }

    let fields = json!({
        "name": name,
        "rate": rate.to_fixed(6),
        "scope": scope_word,
        "collected_account": collected,
        "reclaim_account": reclaim,
        "applies_after": after,
        "effective_from": from,
        "effective_to": until,
    });
    let records = Records::new(graph, TAX_RATE);
    let saved = if id.is_empty() {
        records.create(scope, &fields)?
    } else {
        records.update(scope, &id, &fields)?
    };
    Ok(json!({
        "status": "saved",
        "tax": Rule::from_record(&saved).map(|rule| rule.to_json()).unwrap_or(saved),
    }))
}

/// A far date, so an open-ended rate can be compared against one that has an end.
fn until_or_forever(until: &str) -> &str {
    if until.is_empty() {
        "9999-12-31"
    } else {
        until
    }
}

/// Whether following `from`'s own chain ever arrives at `wanted`, which is how a loop is found before it is saved.
fn chain_reaches(existing: &[Rule], from: &str, wanted: &str, depth: usize) -> bool {
    if depth > 64 {
        return true;
    }
    let Some(rule) = existing.iter().find(|rule| rule.name == from) else {
        return false;
    };
    rule.after
        .iter()
        .any(|next| next == wanted || chain_reaches(existing, next, wanted, depth + 1))
}

/// `POST /accounting/taxes/{id}/retire`: stop a tax applying from a day, keeping what it already posted.
pub fn retire_tax<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let id = text(payload, "id");
    let name = text(payload, "name");
    if id.is_empty() && name.is_empty() {
        return Err(refused("Say which tax to retire, by its name."));
    }
    let until = text(payload, "effective_to");
    if !is_day(&until) {
        return Err(refused(
            "Say the last day the tax applies, as a date like 2026-12-31.",
        ));
    }
    let rules = all_rules(graph, scope)?;
    let rule = rules
        .iter()
        .find(|rule| {
            (!id.is_empty() && rule.id == id)
                || (id.is_empty() && rule.name == name && rule.until.is_empty())
        })
        .ok_or_else(|| {
            refused(&format!(
                "This Book has no tax called {name} still in force, so there is nothing to retire."
            ))
        })?;
    if until.as_str() < rule.from.as_str() {
        return Err(refused(&format!(
            "{} starts on {}, so it cannot stop before that.",
            rule.name, rule.from
        )));
    }
    Records::new(graph, TAX_RATE).update(scope, &rule.id, &json!({ "effective_to": until }))?;
    Ok(json!({
        "status": "retired",
        "tax": rule.name,
        "effective_to": until,
        "note": "What it already posted keeps its rate. The ledger is corrected by reversing an entry, never by editing one.",
    }))
}

/// `POST /accounting/taxes/seed`: the usual taxes, with the accounts the Book already has.
///
/// Seeded so a new Book works on its first day, and never forced: a Book that has defined its own VAT keeps it, because
/// the app has no business changing a rate somebody set. The rates here are a starting point to confirm against ZIMRA.
pub fn seed_taxes<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    _payload: &Value,
) -> Result<Value, StoreError> {
    let chart = ledger_book::accounts(graph, scope)?;
    let named = |wanted: &[&str], kind: &str| -> String {
        for account in &chart {
            let name = field(account, "name").to_lowercase();
            let number = field(account, "account_number");
            if number.is_empty() || !wanted.iter().any(|one| name.contains(one)) {
                continue;
            }
            if kind.is_empty() || field(account, "account_type") == kind {
                return number.to_string();
            }
        }
        String::new()
    };
    let vat = named(&["vat"], "liability");
    let vat_any = if vat.is_empty() {
        named(&["vat", "tax"], "")
    } else {
        vat.clone()
    };
    let reclaim_named = named(&["reclaim", "input"], "");
    // a Book that keeps one tax account reclaims into it; a Book with a separate reclaim account uses that
    let reclaim = if reclaim_named.is_empty() {
        vat_any.clone()
    } else {
        reclaim_named
    };
    let withheld = named(&["withholding", "wht"], "");
    let holding = if !withheld.is_empty() {
        withheld
    } else {
        vat_any.clone()
    };

    let catalogue = [
        (
            "VAT",
            "15.5",
            "both",
            vat_any.as_str(),
            reclaim.as_str(),
            "2026-01-01",
            "Charged on sales, reclaimed on purchases. Confirm the rate with ZIMRA.",
        ),
        (
            "Withholding on contracts",
            "10",
            "both",
            holding.as_str(),
            holding.as_str(),
            "2026-01-01",
            "Deducted from an invoice by the customer and paid to ZIMRA on your behalf.",
        ),
        (
            "Zero-rated",
            "0",
            "both",
            "",
            "",
            "2026-01-01",
            "A taxable supply charged at nothing, such as an export. It still goes on the return.",
        ),
        (
            "Exempt",
            "0",
            "both",
            "",
            "",
            "2026-01-01",
            "Outside the tax entirely. A different line from zero-rated, and it affects what can be reclaimed.",
        ),
    ];

    let existing = all_rules(graph, scope)?;
    let mut seeded: Vec<Value> = Vec::new();
    let mut kept: Vec<Value> = Vec::new();
    for (name, rate, scope_word, collected, reclaims, from, note) in catalogue {
        if existing.iter().any(|rule| rule.name == name) {
            kept.push(json!({ "name": name, "reason": "This Book already defines it." }));
            continue;
        }
        let mut fields = json!({
            "name": name,
            "rate": rate,
            "scope": scope_word,
            "collected_account": collected,
            "reclaim_account": reclaims,
            "applies_after": [],
            "effective_from": from,
            "effective_to": "",
        });
        if rate != "0" && collected.is_empty() {
            // a tax that takes money with nowhere to put it would be a tax that cannot be charged, so the chart decides
            kept.push(json!({
                "name": name,
                "reason": "This Book's chart has no account to post it to. Add one in Chart of accounts, then seed again.",
            }));
            continue;
        }
        fields["note"] = json!(note);
        let saved = Records::new(graph, TAX_RATE).create(scope, &fields)?;
        seeded.push(json!({
            "name": name,
            "rate": rate,
            "account": collected,
            "note": note,
            "id": field(&saved, "id"),
        }));
    }
    Ok(json!({
        "seeded": seeded,
        "kept": kept,
        "note": "These rates are a starting point. Check them against ZIMRA and change any that differ: from now on this \
                 Book's own rate is what it charges.",
    }))
}

/// The percentage given, whether it arrived as a number or as text.
fn rate_of(payload: &Value) -> Result<Option<Money>, StoreError> {
    let given = match payload.get("rate") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) => text.trim().to_string(),
        Some(_) => {
            return Err(refused(
                "The rate is a number, like 15.5 for fifteen and a half per cent.",
            ))
        }
    };
    if given.is_empty() {
        return Ok(None);
    }
    Money::parse(&given)
        .map(Some)
        .map_err(|_| refused("The rate is a number, like 15.5 for fifteen and a half per cent."))
}

/// `POST /accounting/taxes/compute`: what each tax on a document would be, in order, before anything is posted.
///
/// [net] is the document without tax. Each tax is charged on the net plus the amount of any tax it applies after, so a
/// levy declared after VAT is charged on the VAT-inclusive figure and a tax that declares nothing is charged on the net.
pub fn compute_tax<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let on = text(payload, "date");
    if !is_day(&on) {
        return Err(refused(
            "Say the document's date, as a date like 2026-11-19, so the rate that holds that day is used.",
        ));
    }
    let direction = text(payload, "direction").to_lowercase();
    let sale = match direction.as_str() {
        "sale" | "sales" | "invoice" => true,
        "purchase" | "purchases" | "bill" => false,
        "" => {
            return Err(refused(
                "Say whether this is a sale or a purchase, because the same tax is owed on one and reclaimed on the other.",
            ))
        }
        other => {
            return Err(refused(&format!(
                "A document is a sale or a purchase, not a {other}."
            )))
        }
    };
    let net = money(payload, "net")?;
    if net.is_zero() {
        return Err(refused(
            "A document of nothing carries no tax, so there is nothing to work out.",
        ));
    }
    if net.is_negative() {
        return Err(refused("A document cannot be less than nothing."));
    }

    let in_force = rules_in_force(graph, scope, &on, sale)?;
    let wanted: Vec<String> = payload
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
        .unwrap_or_default();

    if wanted.is_empty() {
        // a document carries no tax until one is chosen: applying every tax in force would charge withholding on a sale
        let available: Vec<Value> = in_force
            .iter()
            .map(|rule| json!({"name": rule.name, "rate": rule.rate.to_fixed(4)}))
            .collect();
        return Ok(json!({
            "date": on,
            "direction": if sale { "sale" } else { "purchase" },
            "net": wire(&net),
            "tax_lines": [],
            "tax": wire(&Money::zero()),
            "gross": wire(&net),
            "available": available,
            "note": if in_force.is_empty() {
                "No tax applies to this document on this date, so it carries none.".to_string()
            } else {
                format!(
                    "A document carries no tax until one is chosen. This Book has {}.",
                    in_force
                        .iter()
                        .map(|rule| format!("{} at {}%", rule.name, rule.rate.to_fixed(4)))
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            },
        }));
    }

    let chosen: Vec<Rule> = if wanted.is_empty() {
        in_force.clone()
    } else {
        let mut picked = Vec::new();
        for name in &wanted {
            match in_force.iter().find(|rule| &rule.name == name) {
                Some(rule) => picked.push(rule.clone()),
                None => {
                    let defined = all_rules(graph, scope)?
                        .into_iter()
                        .find(|rule| &rule.name == name);
                    return Err(refused(&match defined {
                        Some(rule) => format!(
                            "{name} does not apply on {on}: this Book has it from {} to {}.",
                            rule.from,
                            if rule.until.is_empty() {
                                "now".to_string()
                            } else {
                                rule.until.clone()
                            }
                        ),
                        None => format!(
                            "{name} is not a tax this Book has defined. Add it first, or pick one of the taxes it has."
                        ),
                    }));
                }
            }
        }
        ordered(picked)
    };

    if chosen.is_empty() {
        return Ok(json!({
            "date": on,
            "direction": if sale { "sale" } else { "purchase" },
            "net": wire(&net),
            "tax_lines": [],
            "tax": wire(&Money::zero()),
            "gross": wire(&net),
            "note": "No tax applies to this document on this date, so it carries none.",
        }));
    }

    let hundred = Money::from_i64(100);
    let mut lines: Vec<Value> = Vec::new();
    let mut total = Money::zero();
    for rule in &chosen {
        // the base is the net plus every tax this one applies after, which is what makes compounding a declaration
        let mut base = net.clone();
        for earlier in &rule.after {
            if let Some(line) = lines
                .iter()
                .find(|line| line["tax"] == json!(earlier.as_str()))
            {
                let amount = Money::parse(field(line, "amount")).map_err(|_| {
                    refused("A tax amount could not be read back, so the order cannot be trusted.")
                })?;
                base = base.plus(&amount);
            }
        }
        let amount = base.times(&rule.rate).over(&hundred).round(SCALE);
        let account = rule.account(sale);
        if !amount.is_zero() && account.is_empty() {
            return Err(refused(&format!(
                "{} is charged on this document but the Book has no account for it, so the tax would go nowhere. \
                 Add one in Chart of accounts, or set it on the tax.",
                rule.name
            )));
        }
        total = total.plus(&amount);
        lines.push(json!({
            "tax": rule.name,
            "rate": rule.rate.to_fixed(4),
            "base": wire(&base),
            "amount": wire(&amount),
            "account_number": account,
            "side": if sale { "credit" } else { "debit" },
        }));
    }
    Ok(json!({
        "date": on,
        "direction": if sale { "sale" } else { "purchase" },
        "net": wire(&net),
        "tax_lines": lines,
        "tax": wire(&total),
        "gross": wire(&net.plus(&total)),
        "note": "Each tax posts to its own account, so nothing is merged into one figure.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::seed_opening_chart;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn charted() -> MemoryGraph {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        graph
    }

    fn define(graph: &MemoryGraph, fields: Value) -> Result<Value, StoreError> {
        save_tax(graph, &scope(), &fields)
    }

    fn vat(graph: &MemoryGraph) -> Value {
        define(
            graph,
            json!({"name": "VAT", "rate": "15.5", "scope": "both",
                   "collected_account": "2100", "reclaim_account": "2100",
                   "effective_from": "2026-01-01"}),
        )
        .expect("VAT defined")
    }

    #[test]
    fn a_rate_is_saved_exactly_and_reads_back_with_its_accounts() {
        let graph = charted();
        vat(&graph);
        let listed = list_taxes(&graph, &scope(), &json!({"date": "2026-11-19"})).expect("listed");
        assert_eq!(listed["count"], json!(1));
        assert_eq!(listed["taxes"][0]["rate"], json!("15.5000"));
        assert_eq!(listed["taxes"][0]["in_force"], json!(true));
        assert_eq!(listed["taxes"][0]["collected_account"], json!("2100"));
    }

    #[test]
    fn a_tax_with_no_account_is_refused_rather_than_posted_nowhere() {
        let graph = charted();
        let refusal = define(
            &graph,
            json!({"name": "Levy", "rate": "2", "effective_from": "2026-01-01"}),
        )
        .expect_err("no account, no tax");
        assert!(refusal.to_string().contains("which account"));
    }

    #[test]
    fn a_rate_that_is_not_a_percentage_or_a_date_is_refused() {
        let graph = charted();
        for payload in [
            json!({"name": "VAT", "rate": "fifteen", "collected_account": "2100", "effective_from": "2026-01-01"}),
            json!({"name": "VAT", "rate": "150", "collected_account": "2100", "effective_from": "2026-01-01"}),
            json!({"name": "VAT", "rate": "15", "collected_account": "2100", "effective_from": "2026"}),
            json!({"name": "VAT", "rate": "15", "collected_account": "9999", "effective_from": "2026-01-01"}),
        ] {
            assert!(define(&graph, payload).is_err());
        }
    }

    #[test]
    fn a_sale_charges_the_rate_on_the_net_and_names_the_account() {
        let graph = charted();
        vat(&graph);
        let answer = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2026-11-19", "direction": "sale", "net": "1000.00", "taxes": ["VAT"]}),
        )
        .expect("computed");
        assert_eq!(answer["tax"], json!("155.000000"));
        assert_eq!(answer["gross"], json!("1155.000000"));
        assert_eq!(answer["tax_lines"][0]["tax"], json!("VAT"));
        assert_eq!(answer["tax_lines"][0]["side"], json!("credit"));
        assert_eq!(answer["tax_lines"][0]["account_number"], json!("2100"));
    }

    #[test]
    fn a_purchase_reclaims_the_same_rate_on_the_other_side() {
        let graph = charted();
        vat(&graph);
        let answer = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2026-11-19", "direction": "purchase", "net": "400.00", "taxes": ["VAT"]}),
        )
        .expect("computed");
        assert_eq!(answer["tax"], json!("62.000000"));
        assert_eq!(answer["tax_lines"][0]["side"], json!("debit"));
    }

    #[test]
    fn a_levy_declared_after_vat_is_charged_on_the_vat_inclusive_figure() {
        let graph = charted();
        vat(&graph);
        define(
            &graph,
            json!({"name": "Levy", "rate": "2", "scope": "sales",
                   "collected_account": "2100", "effective_from": "2026-01-01",
                   "applies_after": ["VAT"]}),
        )
        .expect("the levy");
        let answer = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2026-11-19", "direction": "sale", "net": "1000.00", "taxes": ["VAT", "Levy"]}),
        )
        .expect("computed");
        // 2% of 1155.00, not of 1000.00: the order is the point
        assert_eq!(answer["tax_lines"][1]["tax"], json!("Levy"));
        assert_eq!(answer["tax_lines"][1]["base"], json!("1155.000000"));
        assert_eq!(answer["tax_lines"][1]["amount"], json!("23.100000"));
        assert_eq!(answer["tax"], json!("178.100000"));
    }

    #[test]
    fn two_taxes_that_would_apply_after_each_other_are_refused() {
        let graph = charted();
        vat(&graph);
        define(
            &graph,
            json!({"name": "Levy", "rate": "2", "collected_account": "2100",
                   "effective_from": "2026-01-01", "applies_after": ["VAT"]}),
        )
        .expect("the levy");
        let refusal = define(
            &graph,
            json!({"name": "VAT", "rate": "15.5", "collected_account": "2100",
                   "effective_from": "2027-01-01", "applies_after": ["Levy"]}),
        )
        .expect_err("a loop");
        assert!(refusal.to_string().contains("already applies after"));
    }

    #[test]
    fn a_rate_change_needs_a_day_of_its_own_and_the_old_rate_keeps_its_documents() {
        let graph = charted();
        vat(&graph);
        // the same name cannot hold twice on the same day
        assert!(define(
            &graph,
            json!({"name": "VAT", "rate": "16", "collected_account": "2100",
                   "effective_from": "2026-06-01"})
        )
        .is_err());
        // retiring it, then a new rate from the next day, keeps both truths
        retire_tax(
            &graph,
            &scope(),
            &json!({"name": "VAT", "effective_to": "2025-12-31"}),
        )
        .expect_err("it does not start before 2026");
        retire_tax(
            &graph,
            &scope(),
            &json!({"name": "VAT", "effective_to": "2026-12-31"}),
        )
        .expect("retired");
        define(
            &graph,
            json!({"name": "VAT", "rate": "16", "collected_account": "2100",
                   "effective_from": "2027-01-01"}),
        )
        .expect("the new rate");

        let old = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2026-11-19", "direction": "sale", "net": "1000.00", "taxes": ["VAT"]}),
        )
        .expect("computed");
        let new = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2027-02-01", "direction": "sale", "net": "1000.00", "taxes": ["VAT"]}),
        )
        .expect("computed");
        assert_eq!(old["tax"], json!("155.000000"));
        assert_eq!(new["tax"], json!("160.000000"));
    }

    #[test]
    fn a_tax_asked_for_on_a_day_it_does_not_hold_is_refused_with_its_dates() {
        let graph = charted();
        vat(&graph);
        let refusal = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2025-06-01", "direction": "sale", "net": "100.00", "taxes": ["VAT"]}),
        )
        .expect_err("not in force");
        assert!(refusal.to_string().contains("2026-01-01"));
    }

    #[test]
    fn seeding_gives_a_new_book_the_usual_taxes_and_never_overwrites_one_it_has() {
        let graph = charted();
        let seeded = seed_taxes(&graph, &scope(), &json!({})).expect("seeded");
        assert!(seeded["seeded"].as_array().expect("rows").len() >= 2);
        // VAT is seeded with the chart's own VAT account, never a number this app chose
        let listed = list_taxes(&graph, &scope(), &json!({})).expect("listed");
        let vat_row = listed["taxes"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row["name"] == json!("VAT"))
            .cloned()
            .expect("VAT seeded");
        assert_eq!(vat_row["collected_account"], json!("2100"));

        // seeding again keeps what exists rather than duplicating it
        let again = seed_taxes(&graph, &scope(), &json!({})).expect("seeded again");
        assert_eq!(again["seeded"].as_array().expect("rows").len(), 0);
        assert!(again["kept"].as_array().expect("rows").len() >= 2);
    }

    #[test]
    fn a_book_that_never_defined_a_tax_answers_that_it_carries_none() {
        let graph = charted();
        let answer = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2026-11-19", "direction": "sale", "net": "100.00"}),
        )
        .expect("computed");
        assert_eq!(answer["tax"], json!("0.000000"));
        assert_eq!(answer["tax_lines"], json!([]));
        assert!(answer["note"].as_str().expect("a note").contains("No tax"));
    }

    #[test]
    fn a_document_with_no_tax_chosen_carries_none_and_is_told_what_it_could_choose() {
        let graph = charted();
        vat(&graph);
        define(
            &graph,
            json!({"name": "Withholding on contracts", "rate": "10",
                   "collected_account": "2100", "effective_from": "2026-01-01"}),
        )
        .expect("withholding");
        let answer = compute_tax(
            &graph,
            &scope(),
            &json!({"date": "2026-11-19", "direction": "sale", "net": "1000.00"}),
        )
        .expect("computed");
        // nothing is assumed: withholding is not charged on every sale just because it exists
        assert_eq!(answer["tax"], json!("0.000000"));
        assert_eq!(answer["tax_lines"], json!([]));
        assert_eq!(answer["available"].as_array().expect("rows").len(), 2);
        assert!(answer["note"]
            .as_str()
            .expect("a note")
            .contains("until one is chosen"));
    }
}
