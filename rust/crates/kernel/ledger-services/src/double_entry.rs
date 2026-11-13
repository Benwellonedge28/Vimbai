#![forbid(unsafe_code)]
//! double-entry-principles-service: the rules of double entry, and the entry that has to satisfy them.
//!
//! Nothing here is stored. The service is a reference: given an account type it says which side it grows
//! on, what category it belongs to and which way its balance reads; given a set of described transactions
//! it suggests the entry that records them.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the account types keep their exact spellings, including `administrative_expense`, which is the name
//!   the enum uses even though the rules catalogue keys it `admin_expense`;
//! * assets and expenses grow on the debit side, liabilities, equity and income on the credit side;
//! * a sale is debited to receivables and credited to revenue, a purchase to expense and payables;
//! * an unrecognised description is not an error - the service answers the amounts it was given and says
//!   the transaction was not recognised.
//!
//! Two answers differ from the Python on purpose:
//!
//! * `/validate-journal-entry` accepted a one cent drift (`abs(debits - credits) < 0.01`). The port holds
//!   `Money`, so the entry either balances exactly or it is refused: a journal a cent out is not a journal
//!   that balances, and rounding drift belongs to the figures before they are posted, not after.
//! * `/account-rules/{account_type}` raised `AttributeError` for every account type, because the examples
//!   catalogue named two members (`CASH`, `EXPENSE`) the enum never had. The port answers the rules and the
//!   examples it was always meant to give.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_ledger_core::DoubleEntryError;
use vimbai_store::amounts::{amount_or_zero, wire};
use vimbai_store::read::{refused, text};
use vimbai_store::StoreError;

/// Every account type the service recognises, in the order its enum declared them.
pub const ACCOUNT_TYPES: [&str; 15] = [
    "current_asset",
    "non_current_asset",
    "fixed_asset",
    "intangible_asset",
    "current_liability",
    "non_current_liability",
    "long_term_liability",
    "equity",
    "capital",
    "revenue",
    "other_income",
    "direct_expense",
    "indirect_expense",
    "administrative_expense",
    "selling_expense",
];

/// The sixteenth type, declared after selling expense, which is why the catalogue lists it last.
pub const FINANCIAL_EXPENSE: &str = "financial_expense";

fn is_expense(account_type: &str) -> bool {
    matches!(
        account_type,
        "direct_expense"
            | "indirect_expense"
            | "administrative_expense"
            | "selling_expense"
            | "financial_expense"
    )
}

fn is_asset(account_type: &str) -> bool {
    matches!(
        account_type,
        "current_asset" | "non_current_asset" | "fixed_asset" | "intangible_asset"
    )
}

/// The side an account type grows on.
pub fn normal_balance(account_type: &str) -> &'static str {
    if is_asset(account_type) || is_expense(account_type) {
        "debit"
    } else {
        "credit"
    }
}

/// The category an account type falls under.
pub fn category(account_type: &str) -> &'static str {
    if is_asset(account_type) {
        "asset"
    } else if matches!(
        account_type,
        "current_liability" | "non_current_liability" | "long_term_liability"
    ) {
        "liability"
    } else if matches!(account_type, "equity" | "capital") {
        "equity"
    } else if matches!(account_type, "revenue" | "other_income") {
        "revenue"
    } else {
        "expense"
    }
}

fn known(account_type: &str) -> bool {
    ACCOUNT_TYPES.contains(&account_type) || account_type == FINANCIAL_EXPENSE
}

fn require_type(raw: &Value) -> Result<String, StoreError> {
    let account_type = raw.as_str().unwrap_or_default().to_owned();
    if !known(&account_type) {
        return Err(refused(&format!("unknown account type: {account_type}")));
    }
    Ok(account_type)
}

/// `POST /validate-journal-entry`.
///
/// The entry is accepted when the debits and the credits are the same amount, exactly. The answer carries
/// both totals and the difference, which is a zero `Money` whenever the entry balances.
/// The kernel's refusal in the words this service answers with.
fn describe(failure: &DoubleEntryError) -> String {
    match failure {
        DoubleEntryError::LineMovesBothSides { line } => {
            format!("line {} moves both a debit and a credit", line + 1)
        }
        DoubleEntryError::LineMovesNothing { line } => format!("line {} moves no money", line + 1),
        DoubleEntryError::InvalidAmount { line } => {
            format!("line {} carries an invalid amount", line + 1)
        }
        DoubleEntryError::TooFewLines { lines } => {
            format!(
                "double entry violation: an entry needs at least two lines, this one has {lines}"
            )
        }
        DoubleEntryError::Unbalanced { debits, credits } => format!(
            "double entry violation: debits ({}) must equal credits ({})",
            Money::from_f64_shortest(*debits)
                .map(|money| money.to_fixed(2))
                .unwrap_or_else(|_| debits.to_string()),
            Money::from_f64_shortest(*credits)
                .map(|money| money.to_fixed(2))
                .unwrap_or_else(|_| credits.to_string())
        ),
    }
}

pub fn validate_entry(payload: &Value) -> Result<Value, StoreError> {
    let entries = payload
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| refused("entries is required"))?;
    if entries.is_empty() {
        return Err(refused("an entry needs at least one line"));
    }

    // nothing here decides double entry: the kernel does, on the amounts themselves, and this handler only reads the
    // payload and translates the answer. One implementation of the rules, so a posting can never be judged twice, two
    // different ways.
    let mut lines = Vec::with_capacity(entries.len());
    for line in entries {
        let line_debit = amount_or_zero(line, "debit")?;
        let line_credit = amount_or_zero(line, "credit")?;
        lines.push((line_debit, line_credit));
    }
    let report = vimbai_ledger_core::validate_money_lines(&lines);
    if let Some(failure) = report
        .errors
        .iter()
        .find(|error| !matches!(error, DoubleEntryError::TooFewLines { .. }))
    {
        return Err(refused(&describe(failure)));
    }
    if !report.valid {
        return Err(refused(
            "double entry violation: an entry needs at least two lines",
        ));
    }
    let mut debit = Money::zero();
    let mut credit = Money::zero();
    for (line_debit, line_credit) in &lines {
        debit = debit.plus(line_debit);
        credit = credit.plus(line_credit);
    }
    let difference = debit.minus(&credit).abs();

    Ok(json!({
        "valid": true,
        "total_debit": wire(&debit),
        "total_credit": wire(&credit),
        "difference": wire(&difference),
        "message": "Journal entry is valid",
    }))
}

/// The debit and credit a movement of `amount` puts on this account type.
fn sides(account_type: &str, increase: bool, amount: &Money) -> (Money, Money) {
    let grows_on_debit = normal_balance(account_type) == "debit";
    if grows_on_debit == increase {
        (amount.clone(), Money::zero())
    } else {
        (Money::zero(), amount.clone())
    }
}

/// `POST /calculate-entry`. The service ignored the `transaction_type` it was given and decided the side
/// from `is_increase` alone; the answer echoes the transaction type back untouched.
pub fn calculate_entry(payload: &Value) -> Result<Value, StoreError> {
    let account_type = require_type(payload.get("account_type").unwrap_or(&Value::Null))?;
    let amount = amount_or_zero(payload, "amount")?;
    let increase = payload
        .get("is_increase")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let (debit, credit) = sides(&account_type, increase, &amount);

    let transaction_type = payload
        .get("transaction_type")
        .map(text_of)
        .unwrap_or_default();

    Ok(json!({
        "account_type": account_type,
        "transaction_type": transaction_type,
        "is_increase": increase,
        "debit": wire(&debit),
        "credit": wire(&credit),
        "normal_balance": normal_balance(&account_type),
        "category": category(&account_type),
    }))
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The sentence rules for an account type, as the service listed them.
fn rules_for(account_type: &str) -> Vec<&'static str> {
    if normal_balance(account_type) == "debit" {
        vec![
            "Increases with Debit",
            "Decreases with Credit",
            "Normal: Debit",
        ]
    } else {
        vec![
            "Increases with Credit",
            "Decreases with Debit",
            "Normal: Credit",
        ]
    }
}

/// The worked examples for an account type. The shipped catalogue named `CASH` and `EXPENSE`, members the
/// enum never had, so every call raised before it could answer; these are the examples it meant to give.
fn examples_for(account_type: &str) -> Value {
    let pairs: &[(&str, bool)] = match category(account_type) {
        "asset" => &[("Purchase asset", true), ("Sell asset", false)],
        "liability" => &[("Incur liability", false), ("Pay liability", true)],
        "equity" => &[("Owner invests", false), ("Owner withdraws", true)],
        "revenue" => &[("Make sale", false), ("Receive income", false)],
        _ => &[("Pay expense", true), ("Expense incurred", true)],
    };
    Value::Array(
        pairs
            .iter()
            .map(|(what, debit)| json!({"transaction": what, "debit": debit}))
            .collect(),
    )
}

/// `GET /account-rules/{account_type}`.
pub fn account_rules(raw: &str) -> Result<Value, StoreError> {
    let account_type = require_type(&Value::String(raw.to_owned()))?;
    Ok(json!({
        "account_type": account_type,
        "category": category(&account_type),
        "normal_balance": normal_balance(&account_type),
        "rules": rules_for(&account_type),
        "examples": examples_for(&account_type),
    }))
}

/// `GET /account-types`.
pub fn account_types() -> Value {
    let mut catalogue: Vec<Value> = ACCOUNT_TYPES
        .iter()
        .map(|name| {
            json!({
                "type": name,
                "category": category(name),
                "normal_balance": normal_balance(name),
            })
        })
        .collect();
    catalogue.push(json!({
        "type": FINANCIAL_EXPENSE,
        "category": category(FINANCIAL_EXPENSE),
        "normal_balance": normal_balance(FINANCIAL_EXPENSE),
    }));
    json!({ "account_types": catalogue })
}

/// One side of a template: the account type, whether it is the debit side, and the account it names.
type TemplateSide = (&'static str, bool, &'static str);

/// A template: its name, what it records, and the two sides it moves.
type Template = (&'static str, &'static str, [TemplateSide; 2]);

/// The seven templates the service shipped, each a description and the two sides it moves.
fn templates() -> Vec<Template> {
    vec![
        (
            "sale",
            "Recording a sale",
            [
                ("current_asset", true, "Debit Customers/Receivables"),
                ("revenue", false, "Credit Sales Revenue"),
            ],
        ),
        (
            "purchase",
            "Recording a purchase",
            [
                ("indirect_expense", true, "Debit Purchases/Expense"),
                ("current_liability", false, "Credit Suppliers/Payables"),
            ],
        ),
        (
            "cash_receipt",
            "Receiving cash",
            [
                ("current_asset", true, "Debit Cash/Bank"),
                ("revenue", false, "Credit Revenue"),
            ],
        ),
        (
            "cash_payment",
            "Paying cash",
            [
                ("indirect_expense", true, "Debit Expense"),
                ("current_asset", false, "Credit Cash/Bank"),
            ],
        ),
        (
            "depreciation",
            "Recording depreciation",
            [
                ("indirect_expense", true, "Debit Depreciation Expense"),
                (
                    "non_current_asset",
                    false,
                    "Credit Accumulated Depreciation",
                ),
            ],
        ),
        (
            "bad_debt",
            "Writing off bad debt",
            [
                ("indirect_expense", true, "Debit Bad Debts Expense"),
                ("current_asset", false, "Credit Accounts Receivable"),
            ],
        ),
        (
            "provision_doubtful_debt",
            "Creating provision for doubtful debts",
            [
                ("indirect_expense", true, "Debit Bad Debts Expense"),
                (
                    "current_asset",
                    false,
                    "Credit Provision for Doubtful Debts",
                ),
            ],
        ),
    ]
}

/// `POST /journal-entries/{entry_type}`.
pub fn journal_template(entry_type: &str) -> Result<Value, StoreError> {
    let found = templates()
        .into_iter()
        .find(|(name, _, _)| *name == entry_type)
        .ok_or_else(|| refused(&format!("template for {entry_type} not found")))?;
    let (_, description, lines) = found;
    let entries: Vec<Value> = lines
        .iter()
        .map(|(account_type, debit, explanation)| {
            json!({
                "account_type": account_type,
                "debit": debit,
                "credit": !debit,
                "explanation": explanation,
            })
        })
        .collect();
    Ok(json!({ "description": description, "entries": entries }))
}

/// A suggested entry: one debit and one credit for `amount`.
fn suggested(
    description: String,
    debit_side: (&str, &str),
    credit_side: (&str, &str),
    amount: &Money,
) -> Value {
    json!({
        "description": description,
        "entries": [
            {
                "account_type": debit_side.0,
                "account_name": debit_side.1,
                "debit": wire(amount),
                "credit": wire(&Money::zero()),
            },
            {
                "account_type": credit_side.0,
                "account_name": credit_side.1,
                "debit": wire(&Money::zero()),
                "credit": wire(amount),
            },
        ],
    })
}

/// `POST /transaction-analyzer`. The description is matched on four words, in the service's order:
/// a purchase of an asset is not a purchase, so the asset guard is checked before the purchase branch runs.
pub fn analyze_transaction(payload: &Value) -> Result<Value, StoreError> {
    let description = text(payload, "description");
    if description.is_empty() {
        // the shipped body read `transaction_description`; both spellings are accepted here
        let alternative = text(payload, "transaction_description");
        return analyze_named(alternative, payload);
    }
    analyze_named(description, payload)
}

fn analyze_named(description: String, payload: &Value) -> Result<Value, StoreError> {
    let amounts = payload.get("amounts").cloned().unwrap_or_else(|| json!({}));
    // the shipped service took `amounts` as a whole object of figures; a caller that sends a single
    // amount alongside the description is understood the same way rather than silently suggesting a nil entry
    let amount = if amounts.is_object() && amounts.get("amount").is_some() {
        amount_or_zero(&amounts, "amount")?
    } else {
        amount_or_zero(payload, "amount")?
    };
    let lower = description.to_lowercase();

    let (entry, recognised) = if lower.contains("sale") || lower.contains("revenue") {
        (
            suggested(
                format!("Sale: {description}"),
                ("current_asset", "Accounts Receivable"),
                ("revenue", "Sales Revenue"),
                &amount,
            ),
            true,
        )
    } else if lower.contains("purchase") && !lower.contains("asset") {
        (
            suggested(
                format!("Purchase: {description}"),
                ("indirect_expense", "Purchases"),
                ("current_liability", "Accounts Payable"),
                &amount,
            ),
            true,
        )
    } else if lower.contains("depreciation") {
        (
            suggested(
                format!("Depreciation: {description}"),
                ("indirect_expense", "Depreciation Expense"),
                ("non_current_asset", "Accumulated Depreciation"),
                &amount,
            ),
            true,
        )
    } else {
        (Value::Null, false)
    };

    if recognised {
        Ok(json!({
            "transaction": description,
            "suggested_entry": entry,
        }))
    } else {
        Ok(json!({
            "transaction": description,
            "message": "Transaction type not recognized. Please provide more details.",
            "amounts": amounts,
        }))
    }
}

/// `GET /accounting-equation`.
pub fn accounting_equation() -> Value {
    json!({
        "equation": "Assets = Liabilities + Equity",
        "explanation": "The fundamental accounting equation states that total assets equal total liabilities plus total equity.",
        "elements": {
            "assets": {"definition": "Resources owned by the business", "normal_balance": "Debit"},
            "liabilities": {"definition": "Amounts owed to others", "normal_balance": "Credit"},
            "equity": {"definition": "Owner's stake in the business", "normal_balance": "Credit"},
        },
        "rules": [
            "Every transaction affects at least two accounts",
            "Total debits must equal total credits",
            "The equation must always balance",
        ],
    })
}

/// `GET /account/{account_type}/balance-type`: which way a balance reads once the two sides are netted.
pub fn balance_type(payload: &Value) -> Result<Value, StoreError> {
    let account_type = require_type(payload.get("account_type").unwrap_or(&Value::Null))?;
    let debit = amount_or_zero(payload, "debit_amount")?;
    let credit = amount_or_zero(payload, "credit_amount")?;
    let net = debit.minus(&credit);
    let reads = if normal_balance(&account_type) == "debit" {
        !net.is_negative()
    } else {
        net.is_negative()
    };
    Ok(json!({
        "account_type": account_type,
        "balance_type": if reads { "Debit Balance" } else { "Credit Balance" },
        "net_balance": wire(&net),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_balanced_entry_is_accepted_and_an_unbalanced_one_is_refused() {
        let balanced = json!({"entries": [
            {"account_code": "1000", "account_name": "Cash", "debit": 250, "credit": 0},
            {"account_code": "4000", "account_name": "Sales", "debit": 0, "credit": 250},
        ]});
        let answer = validate_entry(&balanced).expect("balanced");
        assert_eq!(answer["total_debit"], json!("250.000000"));
        assert_eq!(answer["difference"], json!("0.000000"));

        let out_by_a_cent = json!({"entries": [
            {"account_code": "1000", "account_name": "Cash", "debit": 250, "credit": 0},
            {"account_code": "4000", "account_name": "Sales", "debit": 0, "credit": 249.99},
        ]});
        assert!(validate_entry(&out_by_a_cent).is_err());
    }

    #[test]
    fn an_asset_grows_on_the_debit_side_and_a_liability_on_the_credit() {
        let asset = calculate_entry(&json!({"account_type": "current_asset", "amount": 100}))
            .expect("asset");
        assert_eq!(asset["debit"], json!("100.000000"));
        assert_eq!(asset["credit"], json!("0.000000"));

        let liability =
            calculate_entry(&json!({"account_type": "current_liability", "amount": 100}))
                .expect("liability");
        assert_eq!(liability["debit"], json!("0.000000"));
        assert_eq!(liability["credit"], json!("100.000000"));

        let decrease = calculate_entry(&json!({
            "account_type": "current_asset", "amount": 40, "is_increase": false
        }))
        .expect("decrease");
        assert_eq!(decrease["credit"], json!("40.000000"));
    }

    #[test]
    fn every_account_type_carries_a_category_and_a_normal_side() {
        let catalogue = account_types();
        let listed = catalogue["account_types"].as_array().unwrap();
        assert_eq!(listed.len(), 16);
        let administrative = listed
            .iter()
            .find(|row| row["type"] == json!("administrative_expense"))
            .expect("administrative expense");
        assert_eq!(administrative["category"], json!("expense"));
        assert_eq!(administrative["normal_balance"], json!("debit"));
        assert_eq!(listed.last().unwrap()["type"], json!("financial_expense"));
    }

    #[test]
    fn the_rules_answer_for_every_type_instead_of_raising() {
        for account_type in ACCOUNT_TYPES.iter().chain([&FINANCIAL_EXPENSE]) {
            let rules = account_rules(account_type).expect("rules");
            assert_eq!(rules["account_type"], json!(account_type));
            assert!(!rules["examples"].as_array().unwrap().is_empty());
        }
    }

    #[test]
    fn a_sale_is_debited_to_receivables_and_a_purchase_to_payables() {
        let sale = analyze_transaction(
            &json!({"description": "sale of goods", "amounts": {"amount": 120}}),
        )
        .expect("sale");
        assert_eq!(
            sale["suggested_entry"]["entries"][0]["account_name"],
            json!("Accounts Receivable")
        );
        assert_eq!(
            sale["suggested_entry"]["entries"][0]["debit"],
            json!("120.000000")
        );
        assert_eq!(
            sale["suggested_entry"]["entries"][1]["credit"],
            json!("120.000000")
        );

        let purchase = analyze_transaction(
            &json!({"description": "purchase of stock", "amounts": {"amount": 80}}),
        )
        .expect("purchase");
        assert_eq!(
            purchase["suggested_entry"]["entries"][1]["account_name"],
            json!("Accounts Payable")
        );

        let asset = analyze_transaction(
            &json!({"description": "purchase of asset", "amounts": {"amount": 5}}),
        )
        .expect("asset purchase");
        assert_eq!(
            asset["message"],
            json!("Transaction type not recognized. Please provide more details.")
        );
    }

    #[test]
    fn the_seven_templates_keep_their_two_sides() {
        let sale = journal_template("sale").expect("sale template");
        assert_eq!(sale["entries"].as_array().unwrap().len(), 2);
        assert_eq!(sale["entries"][0]["account_type"], json!("current_asset"));
        assert!(journal_template("nothing_like_this").is_err());
    }
}
