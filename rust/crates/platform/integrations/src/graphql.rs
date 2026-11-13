#![forbid(unsafe_code)]
//! graphql-service: one POST body carrying a query, answered field by field.
//!
//! The service is a facade, and it was a facade when it shipped: every query answered fixed sample rows, the arguments
//! offered on those fields were accepted and ignored, and every mutation answered success without writing anything. Ported
//! as it stands rather than dressed up as something it never was:
//!
//! * the sample rows are the shipped rows, field for field;
//! * the filters (`account_type`, `is_active`, `status`, `severity`, `unread_only`, `limit`) are still accepted and still
//!   change nothing, because applying them would answer differently from the service being replaced;
//! * no mutation touches a record. `create_account`, `create_transaction`, `update_account` and the two alert doors answer
//!   the ids the shipped service answered, which are not ids of anything;
//! * `exchange_rate` answers from the same six-pair table, and null for any other pair;
//! * `convert_currency` still converts an unknown pair at 1.0, which is the shipped behaviour and a genuine hazard: it
//!   turns a conversion into a copy without saying so. It is pinned by a test so it cannot be mistaken for an oversight,
//!   and it is worth fixing on the day this door reads real rates.
//!
//! One deliberate change: `create_journal_entry` refuses anything not exactly balanced. The shipped check allowed a tenth
//! of a cent of drift, and the ledger kernel answers the same mutation exactly, so a facade that tolerates drift would be
//! the one door in the platform where a journal entry need not balance.
//!
//! What this crate implements is the doorway, not the schema library: an operation is read, its fields are read with their
//! arguments and their sub-selections, and each field is answered from the fixed table. Fragments and aliases are refused
//! by name rather than answered wrongly, so a client cannot be handed a figure under a name it did not ask for.

use serde_json::{json, Map, Value};

use vimbai_common::dates;
use vimbai_common::{clock, Money};
use vimbai_store::StoreError;

/// The currency pairs the shipped service knew, and the rate between each.
const RATES: [(&str, &str, &str); 6] = [
    ("USD", "EUR", "0.92"),
    ("USD", "GBP", "0.79"),
    ("EUR", "USD", "1.09"),
    ("GBP", "USD", "1.27"),
    ("EUR", "GBP", "0.86"),
    ("GBP", "EUR", "1.16"),
];

fn rate_between(from: &str, to: &str) -> Option<Money> {
    RATES
        .iter()
        .find(|(one, two, _)| *one == from && *two == to)
        .map(|(_, _, rate)| Money::parse(rate))
        .transpose()
        .ok()
        .flatten()
}

/// `POST /graphql`: read one operation and answer it.
pub fn graphql_request(payload: &Value) -> Result<Value, StoreError> {
    let query = payload
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if query.trim().is_empty() {
        return Err(StoreError::Invalid("a query is required".into()));
    }
    let variables = payload
        .get("variables")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let operation = match Operation::read(query, &variables) {
        Ok(operation) => operation,
        // a query that cannot be read is answered with an error beside it, as the shipped door did
        Err(said) => return Ok(json!({"data": Value::Null, "errors": [{"message": said}]})),
    };

    let mut data = Map::new();
    let mut errors: Vec<Value> = Vec::new();
    for field in &operation.fields {
        match answer(&operation.kind, field) {
            Ok((value, type_name)) => {
                data.insert(
                    field.name.clone(),
                    project(&value, &field.children, type_name),
                );
            }
            Err(said) => errors.push(json!({"message": said, "path": [field.name.clone()]})),
        }
    }

    let mut answer = Map::new();
    answer.insert("data".to_string(), Value::Object(data));
    if !errors.is_empty() {
        answer.insert("errors".to_string(), Value::Array(errors));
    }
    Ok(Value::Object(answer))
}

// ---------------------------------------------------------------------------
// Reading the operation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Field {
    name: String,
    arguments: Map<String, Value>,
    children: Vec<Field>,
}

impl Field {
    fn argued(&self, name: &str) -> Option<&Value> {
        self.arguments.get(name)
    }

    fn named(&self, name: &str) -> String {
        self.argued(name)
            .map(|value| match value {
                Value::String(written) => written.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    }
}

struct Operation {
    kind: String,
    fields: Vec<Field>,
}

struct Cursor<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            at: 0,
        }
    }

    fn rest(&self) -> &str {
        std::str::from_utf8(&self.text[self.at..]).unwrap_or_default()
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn skip_ignored(&mut self) {
        loop {
            match self.peek() {
                Some(byte) if byte.is_ascii_whitespace() || byte == b',' => self.at += 1,
                // a comment runs to the end of the line
                Some(b'#') => {
                    while let Some(byte) = self.peek() {
                        self.at += 1;
                        if byte == b'\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn take_if(&mut self, wanted: u8) -> bool {
        self.skip_ignored();
        if self.peek() == Some(wanted) {
            self.at += 1;
            return true;
        }
        false
    }

    fn expect(&mut self, wanted: u8, said: &str) -> Result<(), String> {
        if self.take_if(wanted) {
            return Ok(());
        }
        Err(format!("{said} (at {})", self.rest()))
    }

    fn name(&mut self) -> Result<String, String> {
        self.skip_ignored();
        let start = self.at;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                self.at += 1;
            } else {
                break;
            }
        }
        if start == self.at {
            return Err(format!("a name was expected (at {})", self.rest()));
        }
        Ok(String::from_utf8_lossy(&self.text[start..self.at]).into_owned())
    }

    /// A selection set: the fields between a pair of braces.
    fn selection(&mut self) -> Result<Vec<Field>, String> {
        self.expect(b'{', "a selection was expected")?;
        let mut fields = Vec::new();
        loop {
            self.skip_ignored();
            if self.take_if(b'}') {
                return Ok(fields);
            }
            if self.rest().starts_with("...") {
                return Err(
                    "fragments are not supported by this door; ask for the fields directly".into(),
                );
            }
            fields.push(self.field()?);
        }
    }

    fn field(&mut self) -> Result<Field, String> {
        let name = self.name()?;
        // an alias would answer a figure under a name the client chose; refused rather than guessed at
        if self.rest().trim_start().starts_with(':') {
            return Err(
                "aliases are not supported by this door; ask for the field by its own name".into(),
            );
        }
        let mut arguments = Map::new();
        if self.take_if(b'(') {
            loop {
                self.skip_ignored();
                if self.take_if(b')') {
                    break;
                }
                let argument = self.name()?;
                self.expect(b':', "an argument needs a value")?;
                let value = self.value()?;
                arguments.insert(argument, value);
            }
        }
        let children = if self.rest().trim_start().starts_with('{') {
            self.selection()?
        } else {
            Vec::new()
        };
        Ok(Field {
            name,
            arguments,
            children,
        })
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_ignored();
        match self.peek() {
            Some(b'$') => {
                self.at += 1;
                let named = self.name()?;
                Err(format!(
                    "the variable ${named} was used but this door reads arguments as written; send the value in the query"
                ))
            }
            Some(b'"') => self.string(),
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ignored();
                    if self.take_if(b']') {
                        return Ok(Value::Array(items));
                    }
                    items.push(self.value()?);
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut held = Map::new();
                loop {
                    self.skip_ignored();
                    if self.take_if(b'}') {
                        return Ok(Value::Object(held));
                    }
                    let key = self.name()?;
                    self.expect(b':', "a field needs a value")?;
                    held.insert(key, self.value()?);
                }
            }
            Some(byte) if byte.is_ascii_digit() || byte == b'-' => {
                let start = self.at;
                while let Some(byte) = self.peek() {
                    if byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'+' | b'e' | b'E') {
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                let written = String::from_utf8_lossy(&self.text[start..self.at]).into_owned();
                Ok(match written.parse::<i64>() {
                    Ok(whole) => json!(whole),
                    Err(_) => match written.parse::<f64>() {
                        Ok(fraction) => json!(fraction),
                        Err(_) => return Err(format!("{written} is not a number")),
                    },
                })
            }
            _ => {
                let written = self.name()?;
                Ok(match written.as_str() {
                    "true" => json!(true),
                    "false" => json!(false),
                    "null" => Value::Null,
                    other => match other.parse::<i64>() {
                        Ok(whole) => json!(whole),
                        Err(_) => match other.parse::<f64>() {
                            Ok(fraction) => json!(fraction),
                            // an enum name, which is carried as the text it was written as
                            Err(_) => json!(other),
                        },
                    },
                })
            }
        }
    }

    fn string(&mut self) -> Result<Value, String> {
        self.at += 1;
        let mut written = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err("a string was never closed".into());
            };
            self.at += 1;
            match byte {
                b'"' => return Ok(Value::String(written)),
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return Err("a string ended inside an escape".into());
                    };
                    self.at += 1;
                    written.push(match escape {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        other => other as char,
                    });
                }
                other => written.push(other as char),
            }
        }
    }
}

impl Operation {
    fn read(query: &str, variables: &Value) -> Result<Self, String> {
        // the variables a caller sends are folded into the query's own text so an argument reads the same either way
        let expanded = fold_variables(query, variables)?;
        let mut cursor = Cursor::new(&expanded);
        cursor.skip_ignored();
        let mut kind = "query".to_string();
        if let Some(word) = cursor.rest().split_whitespace().next() {
            if matches!(word, "query" | "mutation" | "subscription") {
                kind = cursor.name()?;
                if kind == "subscription" {
                    return Err("this door does not keep a subscription open".into());
                }
                // an operation may be named, and may declare its own variables before its selection
                if cursor
                    .rest()
                    .trim_start()
                    .starts_with(|symbol: char| symbol.is_ascii_alphabetic())
                {
                    let _ = cursor.name()?;
                }
                if cursor.rest().trim_start().starts_with('(') {
                    cursor.expect(b'(', "the declared variables were expected")?;
                    let mut depth = 1;
                    while let Some(byte) = cursor.peek() {
                        cursor.at += 1;
                        match byte {
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        let fields = cursor.selection()?;
        if fields.is_empty() {
            return Err("a query has to ask for something".into());
        }
        Ok(Self { kind, fields })
    }
}

/// Replace `$name` where it stands with the variable the caller sent, and refuse one that was not sent.
fn fold_variables(query: &str, variables: &Value) -> Result<String, String> {
    let mut written = String::with_capacity(query.len());
    let mut rest = query;
    while let Some(at) = rest.find('$') {
        written.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let end = after
            .find(|symbol: char| !(symbol.is_ascii_alphanumeric() || symbol == '_'))
            .unwrap_or(after.len());
        let named = &after[..end];
        let behind = after[end..].trim_start();
        // `$name:` is the declaration, not a use: it stays as written
        if behind.starts_with(':') {
            written.push('$');
            written.push_str(named);
            rest = &after[end..];
            continue;
        }
        let held = variables.get(named).ok_or_else(|| {
            format!("the variable ${named} was declared but no value was sent for it")
        })?;
        written.push_str(&as_graphql(held));
        rest = &after[end..];
    }
    written.push_str(rest);
    Ok(written)
}

/// A variable's own value, written the way a query would have written it.
fn as_graphql(value: &Value) -> String {
    match value {
        Value::String(written) => format!("\"{}\"", written.replace('"', "\\\"")),
        Value::Null => "null".to_string(),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(as_graphql).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(held) => format!(
            "{{{}}}",
            held.iter()
                .map(|(key, inner)| format!("{key}: {}", as_graphql(inner)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Answering a field
// ---------------------------------------------------------------------------

fn answer(kind: &str, field: &Field) -> Result<(Value, &'static str), String> {
    let asked = field.name.as_str();
    match (kind, asked) {
        ("mutation", named) => mutation(named, field),
        (_, named) => query(named, field),
    }
}

fn query(asked: &str, field: &Field) -> Result<(Value, &'static str), String> {
    let now = written_now();
    let answered = match asked {
        // three accounts' worth of sample rows, and the filters are accepted and ignored as shipped
        "accounts" => (
            json!([
                {"id": "acc-001", "name": "Cash Account", "account_number": "1001", "account_type": "asset",
                 "normal_balance": "debit", "balance": 50000.0, "is_active": true, "created_at": now},
                {"id": "acc-002", "name": "Accounts Receivable", "account_number": "1100", "account_type": "asset",
                 "normal_balance": "debit", "balance": 25000.0, "is_active": true, "created_at": now}
            ]),
            "AccountType",
        ),
        "account" => {
            let id = field.named("id");
            (
                json!({"id": id, "name": "Sample Account", "account_number": "1001", "account_type": "asset",
                       "normal_balance": "debit", "balance": 50000.0, "is_active": true, "created_at": now}),
                "AccountType",
            )
        }
        "journal_entries" => (
            json!([journal_sample("je-001", &now, true)]),
            "JournalEntry",
        ),
        "journal_entry" => {
            let id = field.named("id");
            (journal_sample(&id, &now, false), "JournalEntry")
        }
        "transactions" => (
            json!([{
                "id": "txn-001", "account_id": "acc-001", "transaction_id": "TXN-2024-001", "amount": 1500.0,
                "transaction_type": "transfer", "sender_account_id": "acc-001", "recipient_account_id": "acc-002",
                "timestamp": now, "status": "completed", "fraud_score": 0.15, "fraud_flag": "safe"
            }]),
            "Transaction",
        ),
        "financial_statements" => (
            json!([{
                "id": "fs-001",
                "statement_type": field.named("statement_type"),
                "start_date": field.named("start_date"),
                "end_date": field.named("end_date"),
                "content": "{}",
                "generated_by": "system"
            }]),
            "FinancialStatement",
        ),
        "budgets" => (
            json!([{
                "id": "bud-001", "name": "Marketing Budget 2024", "category": "marketing",
                "allocated_amount": 50000.0, "spent_amount": 25000.0, "remaining_amount": 25000.0, "period": "2024"
            }]),
            "Budget",
        ),
        "alerts" => (
            json!([{
                "id": "alert-001", "title": "High Value Transaction", "message": "Transaction of $15,000 detected",
                "severity": "high", "category": "fraud", "status": "active", "created_at": now
            }]),
            "Alert",
        ),
        "notifications" => (
            json!([{
                "id": "notif-001", "user_id": field.named("user_id"), "type": "approval_required",
                "title": "Approval Needed", "message": "Please approve expense report", "priority": "high",
                "is_read": false, "created_at": now
            }]),
            "Notification",
        ),
        "currencies" => (
            json!([
                {"code": "USD", "name": "US Dollar", "symbol": "$", "exchange_rate_to_usd": 1.0},
                {"code": "EUR", "name": "Euro", "symbol": "€", "exchange_rate_to_usd": 0.92},
                {"code": "GBP", "name": "British Pound", "symbol": "£", "exchange_rate_to_usd": 0.79}
            ]),
            "Currency",
        ),
        "exchange_rate" => {
            let from = field.named("from_currency").to_uppercase();
            let to = field.named("to_currency").to_uppercase();
            let held = rate_between(&from, &to);
            (
                match held {
                    Some(rate) => json!(rate.round(6).to_f64_lossy()),
                    // a pair the service never knew has no rate, which is not the same as a rate of one
                    None => Value::Null,
                },
                "Float",
            )
        }
        "health" => (
            json!({"status": "healthy", "service": "graphql-api"}),
            "Health",
        ),
        other => return Err(format!("the field {other} is not part of this schema")),
    };
    Ok(answered)
}

fn journal_sample(id: &str, now: &str, whole: bool) -> Value {
    let lines = if whole {
        json!([
            {"account_number": "1001", "description": "Cash", "debit": 1000.0, "credit": 0.0},
            {"account_number": "4000", "description": "Revenue", "debit": 0.0, "credit": 1000.0}
        ])
    } else {
        json!([{"account_number": "1001", "description": "Cash", "debit": 1000.0, "credit": 0.0}])
    };
    json!({
        "id": id,
        "entry_date": now,
        "description": "Sample journal entry",
        "source_module": "manual",
        "lines": lines,
        "total_debit": 1000.0,
        "total_credit": 1000.0,
        "is_balanced": true,
        "created_by": "admin"
    })
}

fn mutation(asked: &str, field: &Field) -> Result<(Value, &'static str), String> {
    let answered = match asked {
        // nothing is written, and the ids answered are not ids of anything - as shipped
        "create_account" => (
            json!({"success": true, "message": "Account created", "id": "acc-new-001"}),
            "MutationResult",
        ),
        "update_account" => (
            json!({"success": true, "message": "Account updated", "id": Value::Null}),
            "MutationResult",
        ),
        "create_journal_entry" => (create_journal_entry(field)?, "MutationResult"),
        "create_transaction" => (
            json!({"success": true, "message": "Transaction created", "id": "txn-new-001"}),
            "MutationResult",
        ),
        "convert_currency" => (convert_currency(field)?, "ConversionResult"),
        "acknowledge_alert" => (
            json!({"success": true, "message": "Alert acknowledged", "id": Value::Null}),
            "MutationResult",
        ),
        "resolve_alert" => (
            json!({"success": true, "message": "Alert resolved", "id": Value::Null}),
            "MutationResult",
        ),
        "mark_notification_read" => (
            json!({"success": true, "message": "Notification marked as read", "id": Value::Null}),
            "MutationResult",
        ),
        other => return Err(format!("the mutation {other} is not part of this schema")),
    };
    Ok(answered)
}

/// A journal entry must balance to the cent - exactly, because the ledger does.
fn create_journal_entry(field: &Field) -> Result<Value, String> {
    let given = field.argued("input").cloned().unwrap_or(Value::Null);
    let lines = given
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut debits = Money::zero();
    let mut credits = Money::zero();
    for line in &lines {
        debits = debits.plus(&amount(line, "debit")?);
        credits = credits.plus(&amount(line, "credit")?);
    }
    if debits != credits {
        return Ok(json!({
            "success": false,
            "message": format!(
                "Journal entry is not balanced. Debits: {}, Credits: {}",
                debits.round(2).to_fixed(2),
                credits.round(2).to_fixed(2)
            ),
            "id": Value::Null
        }));
    }
    Ok(json!({"success": true, "message": "Journal entry created", "id": "je-new-001"}))
}

fn amount(held: &Value, key: &str) -> Result<Money, String> {
    match held.get(key) {
        None | Some(Value::Null) => Ok(Money::zero()),
        Some(value) => Money::parse(&match value {
            Value::String(written) => written.clone(),
            other => other.to_string(),
        })
        .map_err(|error| format!("{key} is not an amount: {error}")),
    }
}

fn convert_currency(field: &Field) -> Result<Value, String> {
    let given = field.argued("input").cloned().unwrap_or(Value::Null);
    let from = given
        .get("from_currency")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_uppercase();
    let to = given
        .get("to_currency")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_uppercase();
    let amount = amount(&given, "amount")?;
    // shipped behaviour, kept and pinned: a pair the table never knew converts at one, which is a copy said to be a
    // conversion. Worth fixing when this door reads real rates.
    let rate = rate_between(&from, &to).unwrap_or_else(|| Money::from_i64(1));
    Ok(json!({
        "from_currency": from,
        "to_currency": to,
        "original_amount": amount.round(2).to_f64_lossy(),
        "converted_amount": amount.times(&rate).round(2).to_f64_lossy(),
        "rate_used": rate.round(6).to_f64_lossy(),
    }))
}

// ---------------------------------------------------------------------------
// Answering a selection
// ---------------------------------------------------------------------------

/// Cut the answer down to the fields that were asked for, and only those.
fn project(value: &Value, children: &[Field], type_name: &'static str) -> Value {
    match value {
        Value::Object(held) => {
            let mut answered = Map::new();
            for child in children {
                if child.name == "__typename" {
                    answered.insert("__typename".to_string(), json!(type_name));
                } else if let Some(inner) = held.get(&child.name) {
                    answered.insert(
                        child.name.clone(),
                        project(inner, &child.children, type_name),
                    );
                }
                // a field that is not here is simply not answered, which is what a null would say less clearly
            }
            Value::Object(answered)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| project(item, children, type_name))
                .collect(),
        ),
        scalar => scalar.clone(),
    }
}

fn written_now() -> String {
    dates::written_at(clock::moment())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(query: &str) -> Value {
        graphql_request(&json!({"query": query})).expect("an answer")
    }

    #[test]
    fn a_query_answers_the_sample_rows_it_asks_for_and_nothing_else() {
        let answer = ask("{ accounts { id name balance __typename } }");
        assert_eq!(answer["data"]["accounts"][0]["id"], json!("acc-001"));
        assert_eq!(answer["data"]["accounts"][0]["name"], json!("Cash Account"));
        assert_eq!(
            answer["data"]["accounts"][0]["__typename"],
            json!("AccountType")
        );
        // a field nobody asked for is not answered
        assert!(answer["data"]["accounts"][0]
            .get("account_number")
            .is_none());
        assert_eq!(answer["data"]["accounts"].as_array().map(Vec::len), Some(2));

        // the filters are accepted, and change nothing, because they changed nothing when the service shipped
        let filtered = ask(r#"{ accounts(account_type: "liability", is_active: false) { id } }"#);
        assert_eq!(
            filtered["data"]["accounts"].as_array().map(Vec::len),
            Some(2)
        );

        // a nested selection is cut down at every level
        let entry = ask("{ journal_entry(id: \"je-9\") { id lines { account_number debit } } }");
        assert_eq!(entry["data"]["journal_entry"]["id"], json!("je-9"));
        assert!(entry["data"]["journal_entry"]["lines"][0]
            .get("description")
            .is_none());
    }

    #[test]
    fn a_field_outside_the_schema_is_refused_by_name() {
        let answer = ask("{ ledger_balances { total } }");
        assert_eq!(answer["data"], json!({}));
        let complaint = answer["errors"][0]["message"].as_str().unwrap_or_default();
        assert!(complaint.contains("ledger_balances"), "{complaint}");

        // a fragment would answer under names this door does not read, so it is refused rather than guessed at
        let fragment = ask("query { accounts { ... on AccountType { id } } }");
        assert!(fragment["errors"][0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("fragments"));
        // and a query that is not a query at all earns an error rather than an empty answer
        let nonsense = graphql_request(&json!({"query": "  "})).expect_err("a refusal");
        assert!(matches!(nonsense, StoreError::Invalid(_)));
    }

    #[test]
    fn a_rate_is_answered_where_one_is_known_and_nowhere_else() {
        let known = ask("{ exchange_rate(from_currency: \"usd\", to_currency: \"eur\") }");
        assert_eq!(known["data"]["exchange_rate"], json!(0.92));
        // a pair the service never knew has no rate, which is not the same as a rate of one
        let unknown = ask("{ exchange_rate(from_currency: \"USD\", to_currency: \"ZAR\") }");
        assert_eq!(unknown["data"]["exchange_rate"], Value::Null);
    }

    #[test]
    fn a_conversion_of_an_unknown_pair_is_a_copy_and_says_so_in_its_rate() {
        let known = ask(
            r#"mutation { convert_currency(input: {from_currency: "USD", to_currency: "GBP", amount: 100})
            { converted_amount rate_used original_amount } }"#,
        );
        assert_eq!(
            known["data"]["convert_currency"]["converted_amount"],
            json!(79.0)
        );
        assert_eq!(known["data"]["convert_currency"]["rate_used"], json!(0.79));

        // shipped behaviour, pinned: an unknown pair converts at one rather than refusing
        let unknown = ask(
            r#"mutation { convert_currency(input: {from_currency: "USD", to_currency: "ZAR", amount: 100})
            { converted_amount rate_used } }"#,
        );
        assert_eq!(
            unknown["data"]["convert_currency"]["converted_amount"],
            json!(100.0)
        );
        assert_eq!(unknown["data"]["convert_currency"]["rate_used"], json!(1.0));
    }

    #[test]
    fn a_journal_entry_must_balance_to_the_cent_and_nothing_is_written() {
        let balanced = ask(
            r#"mutation { create_journal_entry(input: {entry_date: "2026-11-13", description: "d",
            source_module: "manual", lines: [
                {account_number: "1001", description: "Cash", debit: 100, credit: 0},
                {account_number: "4000", description: "Revenue", debit: 0, credit: 100}]})
            { success message id } }"#,
        );
        assert_eq!(
            balanced["data"]["create_journal_entry"]["success"],
            json!(true)
        );
        assert_eq!(
            balanced["data"]["create_journal_entry"]["id"],
            json!("je-new-001")
        );

        // a hundredth of a cent out is refused, where the shipped check tolerated a tenth of one
        let drifted = ask(
            r#"mutation { create_journal_entry(input: {entry_date: "2026-11-13", description: "d",
            source_module: "manual", lines: [
                {account_number: "1001", description: "Cash", debit: 100.01, credit: 0},
                {account_number: "4000", description: "Revenue", debit: 0, credit: 100}]})
            { success message } }"#,
        );
        assert_eq!(
            drifted["data"]["create_journal_entry"]["success"],
            json!(false)
        );
        let said = drifted["data"]["create_journal_entry"]["message"]
            .as_str()
            .unwrap_or_default();
        assert!(said.contains("not balanced"), "{said}");

        // no mutation writes anything, and the ids answered are not ids of anything
        let made = ask(
            r#"mutation { create_account(input: {name: "n", account_number: "9", account_type: "asset",
            normal_balance: "debit"}) { success id } }"#,
        );
        assert_eq!(made["data"]["create_account"]["id"], json!("acc-new-001"));
        let alerts = ask("{ alerts { id status } }");
        assert_eq!(alerts["data"]["alerts"][0]["status"], json!("active"));
    }

    #[test]
    fn a_variable_is_folded_into_the_argument_it_stands_for() {
        let answer = graphql_request(&json!({
            "query": "query Ledger($who: String) { notifications(user_id: $who) { user_id is_read } }",
            "variables": {"who": "user-1"}
        }))
        .expect("an answer");
        assert_eq!(
            answer["data"]["notifications"][0]["user_id"],
            json!("user-1")
        );
        // the unread-only filter is accepted and ignored, as shipped
        assert_eq!(answer["data"]["notifications"][0]["is_read"], json!(false));

        // a variable with no value sent is refused rather than answered as empty
        let missing = graphql_request(&json!({
            "query": "query ($who: String) { notifications(user_id: $who) { id } }"
        }))
        .expect("an answer");
        assert!(missing["errors"][0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("$who"));
    }
}
