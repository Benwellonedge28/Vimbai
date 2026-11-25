#![forbid(unsafe_code)]
//! A fact stated in a person's words, posted by the engine.
//!
//! A person does not think in debits and credits. They think "I spent 40 on rent out of the bank" or "Chido paid 500 for
//! goods into the bank". Asking them to choose two accounts is asking them to do the bookkeeper's job and to know a chart
//! they never wrote, so the journal screens that did that are being replaced by this: the fact is stated and the posting
//! is derived.
//!
//! The accounts come from the Book's own chart, matched against the words the person used, and nothing here knows the
//! name of any account, currency or tax. When the words match nothing, or match more than one, the fact is refused
//! naming what the Book does hold, so the person chooses from their own chart instead of guessing at ours. Which account
//! holds money is the Book's own answer too: the cash and bank accounts it registered, never a list in code.
//!
//! `rehearse` answers what would be posted without posting it, so a screen can show the entry before it is made. The
//! posting itself goes through the same door every journal uses, so balance, immutability and closed months are checked
//! once, in the ledger, rather than again here.

use serde_json::{json, Value};
use vimbai_store::amounts::wire;
use vimbai_store::read::{refused, text};
use vimbai_store::{BookScope, Graph, StoreError};

use crate::measures;
use crate::subsidiary_journals::{must_be_open, positive, post};

/// What a person can state happened, and what each means.
const KINDS: [(&str, &str); 2] = [
    ("spent", "money went out of the business on something"),
    ("received", "money came into the business for something"),
];

fn kinds_in_words() -> String {
    KINDS
        .iter()
        .map(|(kind, meaning)| format!("\"{kind}\" ({meaning})"))
        .collect::<Vec<String>>()
        .join(" or ")
}

/// The account type a fact reads on the side that is not money.
fn type_of(kind: &str) -> &'static str {
    if kind == "spent" {
        "expense"
    } else {
        "revenue"
    }
}

/// What the person calls the thing that is not money: a cost for money spent, a kind of trade for money received.
fn thing_in_words(kind: &str) -> &'static str {
    if kind == "spent" {
        "cost"
    } else {
        "kind of trade"
    }
}

/// Accounts whose number or name answers to [words], ignoring case and matching inside the name.
fn answering(chart: &measures::Chart, numbers: &[String], words: &str) -> Vec<String> {
    let wanted = words.trim().to_lowercase();
    if wanted.is_empty() {
        return Vec::new();
    }
    let name_of = |number: &String| {
        chart
            .names
            .get(number.as_str())
            .map(|held| held.to_lowercase())
            .unwrap_or_default()
    };
    // a number or a name said exactly settles it: "sales" means Sales, not Sales Returns as well
    let exact: Vec<String> = numbers
        .iter()
        .filter(|number| number.as_str() == wanted || name_of(number) == wanted)
        .cloned()
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let inside: Vec<String> = numbers
        .iter()
        .filter(|number| name_of(number).contains(&wanted))
        .cloned()
        .collect();
    if !inside.is_empty() {
        return inside;
    }
    // then a phrase a person writes as a sentence: "rent for the shop" finds Rent through the word it shares. The words
    // that only join a sentence together are not what a person means, so they never decide an account: without that,
    // "rent for the shop" would answer to any account whose name happens to hold "for".
    let words: Vec<&str> = wanted
        .split(|letter: char| !letter.is_alphanumeric())
        .filter(|word| word.len() >= 3 && !JOINING.contains(word))
        .collect();
    numbers
        .iter()
        .filter(|number| {
            let name = name_of(number);
            !name.is_empty() && words.iter().any(|word| name.contains(word))
        })
        .cloned()
        .collect()
}

/// The numbers of the accounts of one type, in the Book's own order.
fn of_type(chart: &measures::Chart, wanted: &str) -> Vec<String> {
    let mut numbers: Vec<String> = chart
        .types
        .iter()
        .filter(|(_, kind)| kind.as_str() == wanted)
        .map(|(number, _)| number.clone())
        .collect();
    numbers.sort();
    numbers
}

/// The words that join a sentence together rather than name a thing. They are part of how a person speaks, not of what
/// any Book calls its accounts, so a shared one never settles which account a fact belongs to.
const JOINING: [&str; 16] = [
    "the", "and", "for", "from", "with", "was", "were", "been", "into", "onto", "our", "its",
    "his", "her", "they", "that",
];

/// The accounts money can be spent into: the costs, and the things the business keeps, but never the money accounts
/// themselves, since money cannot be spent on money.
fn spending_accounts(chart: &measures::Chart) -> Vec<String> {
    let mut numbers: Vec<String> = chart
        .types
        .iter()
        .filter(|(number, kind)| {
            (kind.as_str() == "expense" || kind.as_str() == "asset")
                && !chart.money.contains(number)
        })
        .map(|(number, _)| number.clone())
        .collect();
    numbers.sort();
    numbers
}

/// Names for a list of numbers, for a refusal that says what the Book holds.
fn named(chart: &measures::Chart, numbers: &[String]) -> String {
    numbers
        .iter()
        .map(|number| {
            let name = chart
                .names
                .get(number.as_str())
                .cloned()
                .unwrap_or_default();
            format!("{number} {name}").trim().to_string()
        })
        .collect::<Vec<String>>()
        .join(", ")
}

/// One account, or a refusal saying why the words did not settle it.
fn settled(
    chart: &measures::Chart,
    numbers: &[String],
    words: &str,
    what: &str,
    example: &str,
) -> Result<String, StoreError> {
    if numbers.is_empty() {
        return Err(refused(&format!(
            "This Book has no {what} to record that against. {example}"
        )));
    }
    if words.trim().is_empty() {
        return Err(refused(&format!(
            "Say which {what} this was. This Book has {}.",
            named(chart, numbers)
        )));
    }
    let found = answering(chart, numbers, words);
    match found.len() {
        1 => Ok(found[0].clone()),
        0 => Err(refused(&format!(
            "This Book has no {what} called \"{words}\". It has {}.",
            named(chart, numbers)
        ))),
        _ => Err(refused(&format!(
            "\"{words}\" answers to more than one {what}: {}. Say which, by its number.",
            named(chart, &found)
        ))),
    }
}

/// The money account the fact moves through, from the Book's own registered cash and bank accounts.
fn money_account(chart: &measures::Chart, payload: &Value) -> Result<String, StoreError> {
    let words = text(payload, "money");
    if chart.money.is_empty() {
        return Err(refused(
            "This Book keeps no cash or bank account yet, so there is nothing for the money to move through. \
             Register the account it does move through, then state this again.",
        ));
    }
    if words.trim().is_empty() {
        if chart.money.len() == 1 {
            return Ok(chart.money[0].clone());
        }
        return Err(refused(&format!(
            "Say which account the money moved through. This Book keeps {}.",
            named(chart, &chart.money)
        )));
    }
    let found = answering(chart, &chart.money, &words);
    match found.len() {
        1 => Ok(found[0].clone()),
        0 => Err(refused(&format!(
            "This Book keeps no cash or bank account called \"{words}\". It keeps {}.",
            named(chart, &chart.money)
        ))),
        _ => Err(refused(&format!(
            "\"{words}\" answers to more than one account: {}. Say which, by its number.",
            named(chart, &found)
        ))),
    }
}

/// `POST /accounting/facts`: a person states what happened, and the engine posts it.
pub fn record_fact<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let what = text(payload, "what").trim().to_lowercase();
    if what.is_empty() {
        return Err(refused(&format!(
            "Say what happened: {}. Each is a fact, and the posting follows from it.",
            kinds_in_words()
        )));
    }
    if !KINDS.iter().any(|(kind, _)| *kind == what) {
        return Err(refused(&format!(
            "A fact is {} here, not \"{what}\".",
            kinds_in_words()
        )));
    }

    let amount = positive(payload, "amount", "The amount")?;
    let entry_date = text(payload, "entry_date");
    must_be_open(graph, scope, &entry_date)?;

    let chart = measures::chart_of(graph, scope)?;
    let money = money_account(&chart, payload)?;
    // money out goes to something used up or something still owned: a cost, or a thing the business now keeps. Both are
    // what a person means by "spent", so both are searched, and neither is guessed at from a list in code.
    let reachable = if what == "spent" {
        spending_accounts(&chart)
    } else {
        of_type(&chart, type_of(&what))
    };
    let other = settled(
        &chart,
        &reachable,
        &text(payload, "account"),
        thing_in_words(&what),
        if what == "spent" {
            "Add the cost to the chart, or say one of the costs it holds."
        } else {
            "Add the kind of trade to the chart, or say one of the kinds it holds."
        },
    )?;

    let money_name = chart.names.get(money.as_str()).cloned().unwrap_or_default();
    let other_name = chart.names.get(other.as_str()).cloned().unwrap_or_default();
    let (debit, credit) = if what == "spent" {
        (other, money)
    } else {
        (money, other)
    };
    let note = text(payload, "note");
    let description = if note.trim().is_empty() {
        if what == "spent" {
            format!("{other_name} paid out of {money_name}")
        } else {
            format!("{other_name} received into {money_name}")
        }
    } else {
        note.trim().to_string()
    };
    let stated = json!({
        "debit": {"number": debit, "name": chart.names.get(debit.as_str()).cloned().unwrap_or_default()},
        "credit": {"number": credit, "name": chart.names.get(credit.as_str()).cloned().unwrap_or_default()},
        "amount": wire(&amount),
        "description": description,
    });

    // a rehearsal answers the same question without touching the ledger, so a screen can show the entry first
    if payload
        .get("rehearse")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(json!({
            "posted": false,
            "what": what,
            "entry_date": entry_date,
            "would_post": stated,
        }));
    }

    let entry_id = post(
        graph,
        scope,
        payload,
        description.clone(),
        &text(payload, "reference_number"),
        "facts",
        &debit,
        &credit,
        &amount,
        &description,
    )?;
    Ok(json!({
        "posted": true,
        "what": what,
        "entry_date": entry_date,
        "entry_id": entry_id,
        "posting": stated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashbook::create_account as open_money_account;
    use crate::chart::seed_opening_chart;
    use crate::ledger_book;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::{Records, LEDGER_ACCOUNT};

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A Book with the opening chart and one bank account registered, which is what a person sets up first.
    fn with_a_bank() -> MemoryGraph {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        open_money_account(
            &graph,
            &scope(),
            &json!({"account_code": "BANK-1", "account_name": "Bank",
                    "account_type": "bank", "account_number": "1010"}),
        )
        .expect("a bank account");
        graph
    }

    fn a_cost(graph: &MemoryGraph, number: &str, name: &str) {
        Records::new(graph, LEDGER_ACCOUNT)
            .create(
                &scope(),
                &json!({"account_number": number, "name": name, "account_type": "expense",
                        "normal_balance": "debit", "created_at": "", "updated_at": ""}),
            )
            .expect("an account");
    }

    #[test]
    fn money_spent_is_posted_as_the_cost_it_was_out_of_the_account_it_left() {
        let graph = with_a_bank();
        let answer = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "40.00",
                    "account": "rent", "money": "bank"}),
        )
        .expect("a fact");

        assert_eq!(answer["posting"]["debit"]["number"], json!("6100"));
        assert_eq!(answer["posting"]["credit"]["number"], json!("1010"));
        assert_eq!(answer["posting"]["amount"], json!("40.000000"));
        assert_eq!(answer["posted"], json!(true));

        // and the ledger holds it, which is the only proof that matters
        let ledger = ledger_book::ledger_report(
            &graph,
            &scope(),
            "6100",
            &json!({"from": "2026-10-01", "to": "2026-10-31"}),
        )
        .expect("a ledger");
        assert_eq!(ledger["total_debit"], json!("40.000000"));
    }

    #[test]
    fn money_received_is_posted_into_the_account_it_landed_in() {
        let graph = with_a_bank();
        let answer = record_fact(
            &graph,
            &scope(),
            &json!({"what": "received", "entry_date": "2026-10-06", "amount": "500.00",
                    "account": "sales", "money": "1010"}),
        )
        .expect("a fact");
        assert_eq!(answer["posting"]["debit"]["number"], json!("1010"));
        assert_eq!(answer["posting"]["credit"]["number"], json!("4000"));
    }

    #[test]
    fn the_words_are_matched_the_way_a_person_writes_them() {
        let graph = with_a_bank();
        for said in ["RENT", "rent for the shop", "6100"] {
            let answer = record_fact(
                &graph,
                &scope(),
                &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "1.00",
                        "account": said}),
            )
            .unwrap_or_else(|error| panic!("{said} should find Rent: {error}"));
            assert_eq!(answer["posting"]["debit"]["number"], json!("6100"));
        }
    }

    #[test]
    fn words_that_match_nothing_are_refused_with_what_the_book_holds() {
        let graph = with_a_bank();
        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "5.00",
                    "account": "aeroplane"}),
        )
        .expect_err("nothing called that");
        let said = error.to_string();
        assert!(said.contains("no cost called \"aeroplane\""), "{said}");
        assert!(said.contains("Rent"), "the refusal names the costs: {said}");
    }

    #[test]
    fn words_that_answer_to_two_accounts_ask_which_rather_than_choosing() {
        let graph = with_a_bank();
        a_cost(&graph, "6200", "Fuel and oil");
        a_cost(&graph, "6300", "Fuel for vehicles");
        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "5.00",
                    "account": "fuel"}),
        )
        .expect_err("two accounts");
        let said = error.to_string();
        assert!(said.contains("more than one cost"), "{said}");
        assert!(said.contains("6200"), "{said}");
        assert!(said.contains("6300"), "{said}");
    }

    #[test]
    fn a_fact_that_does_not_say_which_cost_asks_for_one() {
        let graph = with_a_bank();
        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "5.00"}),
        )
        .expect_err("no cost said");
        assert!(error.to_string().contains("Say which cost"), "{error}");
    }

    #[test]
    fn a_book_keeping_two_money_accounts_asks_which_one() {
        let graph = with_a_bank();
        open_money_account(
            &graph,
            &scope(),
            &json!({"account_code": "TILL-1", "account_name": "Till",
                    "account_type": "cash", "account_number": "1000"}),
        )
        .expect("a till");
        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "5.00",
                    "account": "rent"}),
        )
        .expect_err("two money accounts");
        let said = error.to_string();
        assert!(
            said.contains("which account the money moved through"),
            "{said}"
        );
        assert!(said.contains("Cash"), "{said}");
        assert!(said.contains("Bank"), "{said}");
    }

    #[test]
    fn a_book_with_no_money_account_is_told_to_register_one() {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "5.00",
                    "account": "rent"}),
        )
        .expect_err("no money account");
        assert!(
            error.to_string().contains("keeps no cash or bank account"),
            "{error}"
        );
    }

    #[test]
    fn a_rehearsal_answers_the_posting_and_posts_nothing() {
        let graph = with_a_bank();
        let answer = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "40.00",
                    "account": "rent", "rehearse": true}),
        )
        .expect("a rehearsal");
        assert_eq!(answer["posted"], json!(false));
        assert_eq!(answer["would_post"]["debit"]["number"], json!("6100"));

        let ledger = ledger_book::ledger_report(
            &graph,
            &scope(),
            "6100",
            &json!({"from": "2026-10-01", "to": "2026-10-31"}),
        )
        .expect("a ledger");
        assert_eq!(ledger["total_debit"], json!("0.000000"));
    }

    #[test]
    fn a_closed_month_is_refused_in_words() {
        let graph = with_a_bank();
        crate::periods::close_period(
            &graph,
            &scope(),
            &json!({"period": "2026-09", "closes_on": "2026-09-30", "closed_by": "the owner"}),
        )
        .expect("September closed");
        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-09-20", "amount": "40.00",
                    "account": "rent"}),
        )
        .expect_err("a closed month");
        assert!(
            error.to_string().to_lowercase().contains("closed"),
            "{error}"
        );
    }

    #[test]
    fn an_amount_of_nothing_is_refused() {
        let graph = with_a_bank();
        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "spent", "entry_date": "2026-10-05", "amount": "0",
                    "account": "rent"}),
        )
        .expect_err("nothing");
        assert!(error.to_string().contains("more than nothing"), "{error}");
    }

    #[test]
    fn what_happened_has_to_be_one_of_the_facts_the_engine_knows() {
        let graph = with_a_bank();
        let error = record_fact(&graph, &scope(), &json!({})).expect_err("nothing said");
        assert!(error.to_string().contains("\"spent\""), "{error}");

        let error = record_fact(
            &graph,
            &scope(),
            &json!({"what": "borrowed", "entry_date": "2026-10-05", "amount": "5.00"}),
        )
        .expect_err("not a fact");
        assert!(error.to_string().contains("\"received\""), "{error}");
    }
}
