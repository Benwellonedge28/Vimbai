//! Which of a Book's accounts is for what.
//!
//! Every journal, the suspense ledger, depreciation and the money accounts ask this one question, so they cannot disagree
//! about where a posting goes. The role a Book gives an account decides it, never the account's name; a Book opened before
//! roles existed still answers by name, so an older Book is never made to reconfigure itself to keep working.

use serde_json::Value;
use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, LEDGER_ACCOUNT};

/// The chart a Book keeps, read through the store so every caller reads the same rows.
fn accounts<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, LEDGER_ACCOUNT).list(scope, 2000)
}

/// An account the Book already keeps, named by the caller, or the one its chart sets aside for this purpose.
///
/// Every journal needs the same three answers: is the account the caller named really in this Book's chart, does the chart
/// have an account carrying this role, and what does the chart call an account at all. They live here rather than in each
/// journal so a refusal about a missing account reads the same wherever it comes from.
///
/// The role, not the name, decides which account a journal posts to. A business that calls its sales account "Turnover",
/// or a Book written in another language, must not be told it has no account for sales: the chart says which account is
/// for sales, and a person can move that role to whichever account they keep. A Book opened before roles existed still
/// answers by name, so an older Book is never made to reconfigure itself to keep working.
pub fn account_named<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
    key: &str,
    purpose: &str,
    kind: &str,
) -> Result<String, StoreError> {
    account_for_purpose(graph, scope, &text(payload, key), purpose, kind)
}

/// The account this Book keeps for a purpose: the one the caller named, the one carrying the role, or - for a Book
/// opened before roles existed - the one called this.
///
/// This is the one function that answers "which of this Book's accounts is for this purpose", so every journal, the
/// suspense ledger and the money accounts themselves cannot disagree about it. `kind` is the account type the purpose
/// must sit in, because a purpose found in the wrong element would post to a liability that should be an asset.
pub fn account_for_purpose<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    named: &str,
    purpose: &str,
    kind: &str,
) -> Result<String, StoreError> {
    let accounts = accounts(graph, scope)?;
    if !named.trim().is_empty() {
        if !accounts
            .iter()
            .any(|account| field(account, "account_number") == named.trim())
        {
            return Err(refused(&format!(
                "Account {named} is not in this Book's chart, so nothing can be posted to it."
            )));
        }
        return Ok(named.trim().to_string());
    }
    let wanted = purpose.trim().to_lowercase();
    let held = accounts
        .iter()
        .find(|account| {
            (kind.is_empty() || field(account, "account_type") == kind)
                && field(account, "role").eq_ignore_ascii_case(&wanted)
        })
        .or_else(|| {
            // a Book opened before the chart carried roles: its accounts are still found by what they are called, by the
            // whole purpose first and then by the element it belongs to, so `depreciation_expense` still finds the
            // account an older chart calls Depreciation
            let words = wanted.replace('_', " ");
            let element = words.split(' ').next().unwrap_or(&words).to_string();
            let roleless = |account: &&Value| field(account, "role").trim().is_empty();
            accounts
                .iter()
                .filter(|account| kind.is_empty() || field(account, "account_type") == kind)
                .find(|account| {
                    roleless(account) && account_name(account).to_lowercase().contains(&words)
                })
                .or_else(|| {
                    accounts
                        .iter()
                        .filter(|account| kind.is_empty() || field(account, "account_type") == kind)
                        .find(|account| {
                            roleless(account)
                                && account_name(account).to_lowercase().contains(&element)
                        })
                })
        });
    held.map(|account| field(account, "account_number").to_string())
        .ok_or_else(|| {
            refused(&format!(
                "This Book has no account set aside for {wanted}, so say which account it belongs in."
            ))
        })
}

/// What the chart calls an account: its own records say `name`, a ledger view says `account_name`, and both are one fact.
pub fn account_name(account: &Value) -> String {
    let named = field(account, "account_name");
    if named.trim().is_empty() {
        field(account, "name").to_string()
    } else {
        named.to_string()
    }
}
