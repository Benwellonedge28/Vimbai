//! Finding one thing in a Book.
//!
//! A Book that has been kept for a year holds thousands of entries, and until now the only way to reach one was to remember
//! which screen it was on: a receipt was in the cash book, an invoice in the invoices list, a supplier's bill in what you
//! owe, a correction in the journal. A person who remembers "INV-2" or "that payment to Rent Co" and not which door it went
//! through has nowhere to type it.
//!
//! So this searches what a person actually remembers - the words and the reference on an entry, and the name or number of an
//! account - across the whole Book at once, and says for each hit what it is and where it opens. It reads the same entries
//! and the same chart the rest of the engine reads, so a hit cannot disagree with the screen it opens.
//!
//! What this keeps, and why:
//!
//! * a search is bounded: fifty hits of each kind, newest first, because a search that answers ten thousand rows is a
//!   search nobody reads and a page nobody can draw;
//! * a query of one letter is refused rather than answered: it matches nearly every entry in the Book, which is the same as
//!   answering nothing, and saying so is more useful than a list of everything;
//! * matching is case-insensitive and looks inside the words, not only at their start, because "rent" is how somebody
//!   remembers "Paid Rent Co";
//! * a draft is searchable and says it is a draft, so a person who has not posted an entry yet can still find it;
//! * nothing is added up and nothing is posted: a search answers where something is, not what it means.

use serde_json::{json, Value};
use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, StoreError};

use crate::ledger_book;

/// How many hits of each kind a search answers. Enough to find what was meant, few enough to read.
const AT_MOST: usize = 50;

/// The shortest query worth answering, in letters.
const SHORTEST: usize = 2;

/// Whether a record's text holds the query, ignoring case.
fn holds(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && haystack.to_lowercase().contains(needle)
}

/// `GET /find`: one thing in a Book, by the words or the reference somebody remembers.
pub fn find<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let asked = text(query, "q").trim().to_string();
    if asked.chars().count() < SHORTEST {
        return Err(refused(&format!(
            "Say at least {SHORTEST} letters to search for: one letter matches nearly everything in the Book, which is \
             the same as answering nothing."
        )));
    }
    let needle = asked.to_lowercase();

    // the entries, as the journal and the notes hold them, newest business date first
    let entries = ledger_book::list_entries(graph, scope, &json!({}))?;
    let mut found: Vec<Value> = Vec::new();
    for entry in entries.as_array().into_iter().flatten() {
        let description = field(entry, "description");
        let reference = field(entry, "reference_number");
        let module = field(entry, "source_module");
        let matched = if holds(reference, &needle) {
            "reference"
        } else if holds(description, &needle) {
            "description"
        } else if holds(module, &needle) {
            "source"
        } else {
            continue;
        };
        let status = match field(entry, "status") {
            "posted" => "posted",
            _ => "not posted",
        };
        found.push(json!({
            "kind": "entry",
            "id": field(entry, "id"),
            "title": description,
            "subtitle": format!("{reference} · {} · {status}", field(entry, "entry_date")),
            "matched_on": matched,
            "status": field(entry, "status"),
        }));
        if found.len() >= AT_MOST {
            break;
        }
    }

    // and the chart, because a person looking for "bank" wants the account as readily as the entry
    let mut accounts_found: Vec<Value> = Vec::new();
    for account in ledger_book::accounts(graph, scope)? {
        let name = field(&account, "name");
        let number = field(&account, "account_number");
        let matched = if holds(number, &needle) {
            "number"
        } else if holds(name, &needle) {
            "name"
        } else {
            continue;
        };
        accounts_found.push(json!({
            "kind": "account",
            "id": number,
            "title": name,
            "subtitle": format!("{number} · {}", field(&account, "account_type")),
            "matched_on": matched,
        }));
        if accounts_found.len() >= AT_MOST {
            break;
        }
    }

    Ok(json!({
        "query": asked,
        "entries": found,
        "accounts": accounts_found,
        "count": found.len() + accounts_found.len(),
        "at_most": AT_MOST,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;

    fn book() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = BookScope::new("user-1", "book-1");
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        let dated = json!({"entry_date": "2026-11-02", "created_at": "2026-11-02T09:00:00Z"});
        // a sale, and a cost, both posted
        for (reference, description, debit, credit, figure) in [
            ("INV-2", "Sold a car to Tendai", "1100", "4000", "12000.00"),
            ("PV-7", "Paid Rent Co", "6100", "1010", "300.00"),
        ] {
            crate::subsidiary_journals::post(
                &graph,
                &scope,
                &dated,
                description.to_string(),
                reference,
                "Tests",
                debit,
                credit,
                &vimbai_common::Money::parse(figure).expect("an amount"),
                "a test entry",
            )
            .expect("posted");
        }
        (graph, scope)
    }

    #[test]
    fn an_entry_is_found_by_its_reference_and_says_where_it_is() {
        let (graph, scope) = book();
        let found = find(&graph, &scope, &json!({"q": "INV-2"})).expect("a search");

        assert_eq!(found["query"], json!("INV-2"));
        let hits = found["entries"].as_array().expect("hits");
        assert_eq!(hits.len(), 1, "{found}");
        assert_eq!(hits[0]["matched_on"], json!("reference"));
        assert_eq!(hits[0]["title"], json!("Sold a car to Tendai"));
        assert!(
            hits[0]["subtitle"]
                .as_str()
                .expect("a subtitle")
                .contains("2026-11-02"),
            "a hit says the day: {found}"
        );
        assert_eq!(hits[0]["status"], json!("posted"));
    }

    #[test]
    fn a_search_looks_inside_the_words_and_ignores_case() {
        let (graph, scope) = book();
        let found = find(&graph, &scope, &json!({"q": "rent co"})).expect("a search");
        let hits = found["entries"].as_array().expect("hits");
        assert_eq!(hits.len(), 1, "{found}");
        assert_eq!(hits[0]["matched_on"], json!("description"));

        // the same search in capitals answers the same thing
        let shouted = find(&graph, &scope, &json!({"q": "RENT CO"})).expect("a search");
        assert_eq!(shouted["entries"].as_array().expect("hits").len(), 1);
    }

    #[test]
    fn an_account_is_found_by_its_name_or_its_number() {
        let (graph, scope) = book();
        let by_name = find(&graph, &scope, &json!({"q": "bank"})).expect("a search");
        let accounts = by_name["accounts"].as_array().expect("accounts");
        assert!(
            accounts.iter().any(|hit| hit["title"] == json!("Bank")),
            "{by_name}"
        );
        assert_eq!(accounts[0]["matched_on"], json!("name"));

        let by_number = find(&graph, &scope, &json!({"q": "6100"})).expect("a search");
        let accounts = by_number["accounts"].as_array().expect("accounts");
        assert_eq!(accounts.len(), 1, "{by_number}");
        assert_eq!(accounts[0]["matched_on"], json!("number"));
    }

    #[test]
    fn a_query_of_one_letter_is_refused_rather_than_answering_everything() {
        let (graph, scope) = book();
        let said = find(&graph, &scope, &json!({"q": "r"}))
            .expect_err("one letter")
            .to_string();
        assert!(said.contains("at least 2 letters"), "{said}");

        let nothing = find(&graph, &scope, &json!({}))
            .expect_err("no query")
            .to_string();
        assert!(nothing.contains("at least 2 letters"), "{nothing}");
    }

    #[test]
    fn a_search_answers_nothing_from_another_book() {
        let (graph, _scope) = book();
        let elsewhere = BookScope::new("user-1", "book-2");
        let found = find(&graph, &elsewhere, &json!({"q": "INV-2"})).expect("a search");
        assert_eq!(
            found["count"],
            json!(0),
            "another Book holds nothing: {found}"
        );
    }

    #[test]
    fn a_draft_is_searchable_and_says_it_is_not_posted() {
        let (graph, scope) = book();
        // a note that is not a posting: it lives in the drafts, and the search reads those too
        vimbai_store::Records::new(&graph, vimbai_store::JOURNAL_DRAFT)
            .create(
                &scope,
                &json!({
                    "entry_date": "2026-11-05",
                    "description": "Owed for the yard rent",
                    "reference_number": "DR-1",
                    "status": "draft",
                }),
            )
            .expect("a draft");

        let found = find(&graph, &scope, &json!({"q": "yard rent"})).expect("a search");
        let hits = found["entries"].as_array().expect("hits");
        assert_eq!(hits.len(), 1, "{found}");
        assert_eq!(hits[0]["status"], json!("draft"));
        assert!(
            hits[0]["subtitle"]
                .as_str()
                .expect("a subtitle")
                .contains("not posted"),
            "a draft says it is not posted: {found}"
        );
    }
}
