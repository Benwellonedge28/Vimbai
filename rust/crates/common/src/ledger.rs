#![forbid(unsafe_code)]
//! The one replay of a party's balance from the entries behind it.
//!
//! A ledger of any kind - debtors owed to the business, creditors owed by it - is its entries and nothing
//! else: each party's balance is replayed from them in the order they were recorded. Both ledgers in the
//! codebase would otherwise carry their own copy of that walk, and a difference between the two copies is
//! exactly the kind of drift that makes one ledger disagree with the other about a type it has never seen.
//!
//! The direction an entry takes is the caller's, because the sign of a type belongs to the service that
//! defines it. What is shared here is the walk: an entry that moves nothing moves nothing, a party is only
//! listed once anything has moved, and the order parties first appear is the order they are answered in.

use serde_json::Value;

use crate::money::Money;

/// An entry's amount, whether the store handed it back as the string it writes or as a number.
fn amount(value: Option<&Value>) -> Option<Money> {
    match value? {
        Value::String(text) => Money::parse(text).ok(),
        Value::Number(number) => Money::from_f64_shortest(number.as_f64()?).ok(),
        _ => None,
    }
}

/// Each party's balance, replayed from the entries in the order given.
///
/// `party_field` is the entry's own name for the party (`debtor_id`, `creditor_id`), and `movement` says
/// which way a transaction type moves a balance: positive to add, negative to take away, zero to leave it
/// alone. An entry of zero movement is still an entry the ledger holds, but it neither creates a party nor
/// changes one.
pub fn replayed_balances(
    entries: &[Value],
    party_field: &str,
    movement: impl Fn(&str) -> i8,
) -> Vec<(String, Money)> {
    let mut order: Vec<String> = Vec::new();
    let mut balances: Vec<Money> = Vec::new();
    for entry in entries {
        let party = entry
            .get(party_field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let kind = entry
            .get("transaction_type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let direction = movement(kind);
        let moved = amount(entry.get("amount")).unwrap_or_else(Money::zero);
        match order.iter().position(|seen| *seen == party) {
            Some(at) if direction > 0 => balances[at] = balances[at].plus(&moved),
            Some(at) if direction < 0 => balances[at] = balances[at].minus(&moved),
            Some(_) => {}
            None if direction != 0 => {
                order.push(party);
                balances.push(if direction > 0 { moved } else { moved.negate() });
            }
            None => {}
        }
    }
    order.into_iter().zip(balances).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn movement(kind: &str) -> i8 {
        match kind {
            "invoice" => 1,
            "payment" | "credit_note" => -1,
            _ => 0,
        }
    }

    #[test]
    fn a_partys_balance_is_the_replay_of_its_own_entries() {
        let entries = vec![
            json!({"debtor_id": "d-1", "transaction_type": "invoice", "amount": "1000.000000"}),
            json!({"debtor_id": "d-1", "transaction_type": "payment", "amount": "400.000000"}),
            json!({"debtor_id": "d-2", "transaction_type": "invoice", "amount": "250.500000"}),
        ];
        let balances = replayed_balances(&entries, "debtor_id", movement);
        assert_eq!(balances[0].0, "d-1");
        assert_eq!(balances[0].1.to_fixed(2), "600.00");
        assert_eq!(balances[1].0, "d-2");
        assert_eq!(balances[1].1.to_fixed(2), "250.50");
    }

    #[test]
    fn an_entry_that_moves_nothing_leaves_an_existing_balance_alone() {
        let entries = vec![
            json!({"debtor_id": "d-1", "transaction_type": "invoice", "amount": "1000.000000"}),
            json!({"debtor_id": "d-1", "transaction_type": "memo", "amount": "999.000000"}),
        ];
        let balances = replayed_balances(&entries, "debtor_id", movement);
        assert_eq!(balances.len(), 1);
        assert_eq!(balances[0].1.to_fixed(2), "1000.00");
    }

    #[test]
    fn a_party_whose_only_entries_move_nothing_is_not_listed() {
        let entries =
            vec![json!({"debtor_id": "d-1", "transaction_type": "memo", "amount": "5.000000"})];
        assert!(replayed_balances(&entries, "debtor_id", movement).is_empty());
    }

    #[test]
    fn an_entry_that_takes_away_first_owes_the_negative_of_it() {
        let entries =
            vec![json!({"debtor_id": "d-1", "transaction_type": "payment", "amount": "50.000000"})];
        let balances = replayed_balances(&entries, "debtor_id", movement);
        assert_eq!(balances[0].1.to_fixed(2), "-50.00");
    }
}
