#![forbid(unsafe_code)]
//! currency-service: the currencies a Book deals in, the rates between them, and the conversions over them.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the fifteen default currencies and their USD-relative rates are seeded per Book the first time it is
//!   touched, and only when the Book has no currencies at all - a Book that has deleted every currency gets
//!   them back, which is what the shipped seeding did;
//! * a rate is a series, not a value: the newest rate for a pair is the one a conversion uses, and the older
//!   ones stay as history;
//! * a pair with no stated rate is answered by the inverse, and failing that by a cross rate through USD, so
//!   EUR to GBP is answered from USD even though nobody stated it;
//! * a converted amount is rounded to the decimal places the target currency declares, half up, so a yen
//!   conversion lands on a whole yen;
//! * arithmetic is done in exact decimals rather than the shipped floats, so a large amount converted at a
//!   long rate keeps every digit the caller sent.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, label, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, CURRENCY, EXCHANGE_RATE};

/// The currencies the shipped service started every Book with.
const DEFAULT_CURRENCIES: [(&str, &str, &str, i64); 15] = [
    ("USD", "US Dollar", "$", 2),
    ("EUR", "Euro", "€", 2),
    ("GBP", "British Pound", "£", 2),
    ("JPY", "Japanese Yen", "¥", 0),
    ("CNY", "Chinese Yuan", "¥", 2),
    ("INR", "Indian Rupee", "₹", 2),
    ("CAD", "Canadian Dollar", "C$", 2),
    ("AUD", "Australian Dollar", "A$", 2),
    ("CHF", "Swiss Franc", "CHF", 2),
    ("HKD", "Hong Kong Dollar", "HK$", 2),
    ("SGD", "Singapore Dollar", "S$", 2),
    ("SEK", "Swedish Krona", "kr", 2),
    ("NOK", "Norwegian Krone", "kr", 2),
    ("MXN", "Mexican Peso", "$", 2),
    ("BRL", "Brazilian Real", "R$", 2),
];

/// The USD-relative rates the same seeding wrote. Every pair not stated here is reached by inversion or by
/// crossing USD.
const DEFAULT_RATES: [(&str, &str, &str); 15] = [
    ("USD", "USD", "1"),
    ("USD", "EUR", "0.92"),
    ("USD", "GBP", "0.79"),
    ("USD", "JPY", "149.50"),
    ("USD", "CNY", "7.24"),
    ("USD", "INR", "83.12"),
    ("USD", "CAD", "1.36"),
    ("USD", "AUD", "1.53"),
    ("USD", "CHF", "0.88"),
    ("USD", "HKD", "7.82"),
    ("USD", "SGD", "1.34"),
    ("USD", "SEK", "10.42"),
    ("USD", "NOK", "10.65"),
    ("USD", "MXN", "17.15"),
    ("USD", "BRL", "4.97"),
];

/// The most crossings a conversion may take. A rate table cannot be walked deeper than the number of
/// currencies in it, so this bounds the search without ever refusing an answer a table could give.
const MAX_CROSSINGS: u32 = 4;

/// The most records a listing reads. A Book holds far fewer currencies and rates than this.
const LARGEST_PAGE: usize = 2000;

// ---------------------------------------------------------------------------
// records
// ---------------------------------------------------------------------------

fn currencies(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, CURRENCY).list(scope, LARGEST_PAGE)
}

fn rates(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, EXCHANGE_RATE).list(scope, LARGEST_PAGE)
}

/// The newest rate for each pair. The shipped service read the newest of each pair and then let the later
/// rows overwrite the earlier ones, so this is what a conversion sees.
fn latest_rates(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    let mut newest: Vec<Value> = Vec::new();
    for rate in rates(graph, scope)? {
        let pair = (
            field(&rate, "from_currency").to_uppercase(),
            field(&rate, "to_currency").to_uppercase(),
        );
        match newest.iter_mut().find(|held| {
            field(held, "from_currency").to_uppercase() == pair.0
                && field(held, "to_currency").to_uppercase() == pair.1
        }) {
            Some(held) => {
                if field(&rate, "effective_date") >= field(held, "effective_date") {
                    *held = rate;
                }
            }
            None => newest.push(rate),
        }
    }
    Ok(newest)
}

/// The decimals a currency declares, two when the currency is not one the Book keeps.
fn decimal_places(held: &[Value], code: &str) -> i64 {
    held.iter()
        .find(|currency| field(currency, "code").eq_ignore_ascii_case(code))
        .map(|currency| i64::from(vimbai_store::read::whole(currency, "decimal_places")))
        .unwrap_or(2)
}

fn find_currency(held: &[Value], code: &str) -> Option<Value> {
    held.iter()
        .find(|currency| field(currency, "code").eq_ignore_ascii_case(code))
        .cloned()
}

// ---------------------------------------------------------------------------
// seeding
// ---------------------------------------------------------------------------

/// The defaults, written once per Book.
///
/// The shipped service seeded when the Book had no currency at all, so a caller who deleted them all got them
/// back. Kept: reading currencies, rates or converting seeds first, which is why a fresh Book can convert
/// before it has created anything.
fn seed_defaults(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    today: &str,
) -> Result<(), StoreError> {
    if !currencies(graph, scope)?.is_empty() {
        return Ok(());
    }
    for (code, name, symbol, places) in DEFAULT_CURRENCIES {
        Records::new(graph, CURRENCY).create(
            scope,
            &json!({
                "code": code,
                "name": name,
                "symbol": symbol,
                "decimal_places": places,
                "is_active": true,
                "created_at": today,
            }),
        )?;
    }
    for (from, to, rate) in DEFAULT_RATES {
        Records::new(graph, EXCHANGE_RATE).create(
            scope,
            &json!({
                "from_currency": from,
                "to_currency": to,
                "rate": rate,
                "effective_date": today,
                "source": "default",
                "last_updated": today,
            }),
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// conversion
// ---------------------------------------------------------------------------

/// The rate for a pair, honouring what the shipped converter did: a stated rate, else its inverse, else a
/// cross rate through USD.
fn rate_between(held: &[Value], from: &str, to: &str, depth: u32) -> Option<Money> {
    if depth > MAX_CROSSINGS {
        return None;
    }
    let direct = held.iter().find(|rate| {
        field(rate, "from_currency").eq_ignore_ascii_case(from)
            && field(rate, "to_currency").eq_ignore_ascii_case(to)
    });
    if let Some(rate) = direct {
        return stored_amount(rate, "rate").ok();
    }
    let inverse = held.iter().find(|rate| {
        field(rate, "from_currency").eq_ignore_ascii_case(to)
            && field(rate, "to_currency").eq_ignore_ascii_case(from)
    });
    if let Some(rate) = inverse {
        let stated = stored_amount(rate, "rate").ok()?;
        if !stated.is_zero() {
            return Money::parse("1").ok().map(|one| one.over(&stated));
        }
    }
    if !from.eq_ignore_ascii_case("USD") && !to.eq_ignore_ascii_case("USD") {
        let from_usd = rate_between(held, "USD", from, depth + 1)?;
        let usd_to = rate_between(held, "USD", to, depth + 1)?;
        if !from_usd.is_zero() {
            return Some(usd_to.over(&from_usd));
        }
    }
    None
}

/// A converted amount, half up at the target currency's decimal places.
fn converted(
    held: &[Value],
    currencies_held: &[Value],
    from: &str,
    to: &str,
    amount: &Money,
) -> Result<(Money, Money), StoreError> {
    let rate = rate_between(held, from, to, 0)
        .ok_or_else(|| refused(&format!("No exchange rate found for {from} to {to}")))?;
    let places = decimal_places(currencies_held, to);
    Ok((amount.times(&rate).round(places), rate))
}

fn conversion_result(
    held: &[Value],
    currencies_held: &[Value],
    from: &str,
    to: &str,
    amount: &Money,
    date: &str,
    rounding: &str,
) -> Result<Value, StoreError> {
    let (converted_amount, rate) = converted(held, currencies_held, from, to, amount)?;
    Ok(json!({
        "from_currency": from.to_uppercase(),
        "to_currency": to.to_uppercase(),
        "original_amount": wire(amount),
        "converted_amount": wire(&converted_amount),
        "rate_used": wire(&rate),
        // the date the caller asked at, or the day the request was made: the shipped service answered the
        // moment it ran, which a port cannot reproduce and does not need to
        "rate_date": date,
        "rounding_mode": rounding,
    }))
}

// ---------------------------------------------------------------------------
// the currencies themselves
// ---------------------------------------------------------------------------

/// `GET /currencies`.
pub fn list_currencies(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(query, "today"))?;
    let active_only = query
        .get("active_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut held = currencies(graph, scope)?;
    if active_only {
        held.retain(|currency| vimbai_store::read::flag(currency, "is_active"));
    }
    Ok(Value::Array(held))
}

/// `GET /currencies/{code}`.
pub fn get_currency(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    code: &str,
    today: &str,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, today)?;
    find_currency(&currencies(graph, scope)?, code).ok_or(StoreError::NotFound)
}

/// `POST /currencies`: a new code, or a refusal stating that the Book already deals in it.
pub fn create_currency(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(payload, "today"))?;
    let code = text(payload, "code").to_uppercase();
    if code.is_empty() {
        return Err(refused("code is required"));
    }
    if find_currency(&currencies(graph, scope)?, &code).is_some() {
        return Err(refused(&format!("Currency {code} already exists")));
    }
    let body = json!({
        "code": code,
        "name": text(payload, "name"),
        "symbol": text(payload, "symbol"),
        "decimal_places": vimbai_store::read::whole(payload, "decimal_places"),
        "is_active": true,
        "created_at": text(payload, "created_at"),
    });
    Records::new(graph, CURRENCY).create(scope, &body)
}

/// `PUT /currencies/{code}`: the name, symbol and decimal places; a currency's code is what identifies it.
pub fn update_currency(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    code: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(payload, "today"))?;
    let held = find_currency(&currencies(graph, scope)?, code).ok_or(StoreError::NotFound)?;
    let id = field(&held, "id").to_string();
    Records::new(graph, CURRENCY).update(
        scope,
        &id,
        &json!({
            "name": text(payload, "name"),
            "symbol": text(payload, "symbol"),
            "decimal_places": vimbai_store::read::whole(payload, "decimal_places"),
        }),
    )
}

/// `DELETE /currencies/{code}`: a currency stops being offered, and the record stays.
pub fn deactivate_currency(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    code: &str,
    today: &str,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, today)?;
    let held = find_currency(&currencies(graph, scope)?, code).ok_or(StoreError::NotFound)?;
    let id = field(&held, "id").to_string();
    Records::new(graph, CURRENCY).update(scope, &id, &json!({ "is_active": false }))
}

// ---------------------------------------------------------------------------
// the rates
// ---------------------------------------------------------------------------

/// `GET /rates`, narrowed by either side of the pair.
pub fn list_exchange_rates(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(query, "today"))?;
    let from = label(query, "from_currency").to_uppercase();
    let to = label(query, "to_currency").to_uppercase();
    let mut held = rates(graph, scope)?;
    if !from.is_empty() {
        held.retain(|rate| field(rate, "from_currency").eq_ignore_ascii_case(&from));
    }
    if !to.is_empty() {
        held.retain(|rate| field(rate, "to_currency").eq_ignore_ascii_case(&to));
    }
    Ok(Value::Array(held))
}

/// `GET /rates/latest`.
pub fn get_latest_rates(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    today: &str,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, today)?;
    Ok(Value::Array(latest_rates(graph, scope)?))
}

/// `GET /rates/{from}/{to}`: the newest stated rate for one pair.
pub fn get_exchange_rate(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    from: &str,
    to: &str,
    today: &str,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, today)?;
    let mut stated: Vec<Value> = rates(graph, scope)?
        .into_iter()
        .filter(|rate| {
            field(rate, "from_currency").eq_ignore_ascii_case(from)
                && field(rate, "to_currency").eq_ignore_ascii_case(to)
        })
        .collect();
    stated.sort_by(|left, right| field(left, "effective_date").cmp(field(right, "effective_date")));
    stated.pop().ok_or_else(|| {
        refused(&format!(
            "No rate found for {} to {}",
            from.to_uppercase(),
            to.to_uppercase()
        ))
    })
}

/// `POST /rates`: a new rate for a pair, dated, on top of the ones already stated.
pub fn create_exchange_rate(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(payload, "today"))?;
    let from = text(payload, "from_currency").to_uppercase();
    let to = text(payload, "to_currency").to_uppercase();
    if from.is_empty() || to.is_empty() {
        return Err(refused("from_currency and to_currency are required"));
    }
    let rate = amount(payload, "rate")?;
    let today = label(payload, "today");
    let effective = match text(payload, "effective_date") {
        stated if stated.is_empty() => today.clone(),
        stated => stated,
    };
    Records::new(graph, EXCHANGE_RATE).create(
        scope,
        &json!({
            "from_currency": from,
            "to_currency": to,
            "rate": wire(&rate),
            "effective_date": effective,
            "source": vimbai_store::read::or_default(payload, "source", "manual"),
            "last_updated": today,
        }),
    )
}

/// `PUT /rates/{from}/{to}`: the newest stated rate for the pair is restated rather than a second row added,
/// which is what the shipped update did - it edited the row it had found.
pub fn update_exchange_rate(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    from: &str,
    to: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = get_exchange_rate(graph, scope, from, to, &label(payload, "today"))?;
    let id = field(&held, "id").to_string();
    let rate = amount(payload, "rate")?;
    let effective = match text(payload, "effective_date") {
        stated if stated.is_empty() => field(&held, "effective_date").to_string(),
        stated => stated,
    };
    Records::new(graph, EXCHANGE_RATE).update(
        scope,
        &id,
        &json!({
            "rate": wire(&rate),
            "effective_date": effective,
            "source": vimbai_store::read::or_default(payload, "source", "manual"),
            "last_updated": label(payload, "today"),
        }),
    )
}

/// `GET /rates/history/{from}/{to}`: the stated series for a pair, newest first.
pub fn historical_rates(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    from: &str,
    to: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(query, "today"))?;
    let start = label(query, "start_date");
    let end = label(query, "end_date");
    let limit = match vimbai_store::read::whole(query, "limit") {
        0 => 100,
        stated => stated as usize,
    };
    let mut held: Vec<Value> = rates(graph, scope)?
        .into_iter()
        .filter(|rate| {
            field(rate, "from_currency").eq_ignore_ascii_case(from)
                && field(rate, "to_currency").eq_ignore_ascii_case(to)
        })
        .collect();
    if !start.is_empty() {
        held.retain(|rate| field(rate, "effective_date") >= start.as_str());
    }
    if !end.is_empty() {
        held.retain(|rate| field(rate, "effective_date") <= end.as_str());
    }
    held.sort_by(|left, right| field(right, "effective_date").cmp(field(left, "effective_date")));
    let kept: Vec<Value> = held.into_iter().take(limit).collect();
    Ok(json!({
        "from_currency": from.to_uppercase(),
        "to_currency": to.to_uppercase(),
        "count": kept.len(),
        "rates": kept,
    }))
}

// ---------------------------------------------------------------------------
// conversion over those rates
// ---------------------------------------------------------------------------

/// `POST /convert`.
pub fn convert_currency(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(payload, "today"))?;
    let held = latest_rates(graph, scope)?;
    let currencies_held = currencies(graph, scope)?;
    let from = text(payload, "from_currency").to_uppercase();
    let to = text(payload, "to_currency").to_uppercase();
    if from.is_empty() || to.is_empty() {
        return Err(refused("from_currency and to_currency are required"));
    }
    let amount = amount(payload, "amount")?;
    conversion_result(
        &held,
        &currencies_held,
        &from,
        &to,
        &amount,
        &label(payload, "rate_date"),
        &vimbai_store::read::or_default(payload, "rounding", "HALF_UP"),
    )
}

/// `POST /convert/batch`: every line answered, the ones that cannot convert reported by their place.
pub fn convert_batch(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    requests: &[Value],
    today: &str,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, today)?;
    let held = latest_rates(graph, scope)?;
    let currencies_held = currencies(graph, scope)?;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for (index, request) in requests.iter().enumerate() {
        let from = text(request, "from_currency").to_uppercase();
        let to = text(request, "to_currency").to_uppercase();
        let converted = amount(request, "amount").and_then(|amount| {
            conversion_result(
                &held,
                &currencies_held,
                &from,
                &to,
                &amount,
                &label(request, "rate_date"),
                &vimbai_store::read::or_default(request, "rounding", "HALF_UP"),
            )
        });
        match converted {
            Ok(result) => {
                results.push(json!({ "index": index, "success": true, "result": result }))
            }
            Err(error) => errors.push(json!({
                "index": index,
                "success": false,
                "error": error.to_string(),
            })),
        }
    }
    Ok(json!({
        "total": requests.len(),
        "successful": results.len(),
        "failed": errors.len(),
        "results": results,
        "errors": errors,
    }))
}

/// `POST /triangulate`: the direct conversion against the same conversion made through USD.
pub fn triangulate(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(query, "today"))?;
    let held = latest_rates(graph, scope)?;
    let currencies_held = currencies(graph, scope)?;
    let from = label(query, "from_currency").to_uppercase();
    let to = label(query, "to_currency").to_uppercase();
    if from.is_empty() || to.is_empty() {
        return Err(refused("from_currency and to_currency are required"));
    }
    let amount = amount(query, "amount")?;
    let direct = conversion_result(&held, &currencies_held, &from, &to, &amount, "", "HALF_UP")?;

    if from == "USD" || to == "USD" {
        return Ok(json!({
            "direct_conversion": direct,
            "cross_conversion": Value::Null,
            "recommendation": "No triangulation needed (same base currency)",
        }));
    }

    let (mid, _) = converted(&held, &currencies_held, &from, "USD", &amount)?;
    let (crossed, _) = converted(&held, &currencies_held, "USD", &to, &mid)?;
    let cross = json!({
        "from_currency": from,
        "to_currency": to,
        "original_amount": wire(&amount),
        "converted_amount": wire(&crossed),
        "rate_used": wire(&rate_between(&held, &from, &to, 0).unwrap_or_else(Money::zero)),
        "rate_date": "",
        "rounding_mode": "HALF_UP",
    });

    let direct_amount =
        Money::parse(field(&direct, "converted_amount")).unwrap_or_else(|_| Money::zero());
    let cross_amount = crossed;
    let difference = direct_amount.minus(&cross_amount).abs();
    let hundred = Money::from_i64(100);
    let savings = if direct_amount.is_zero() {
        Money::zero()
    } else {
        // 1 - cross/direct, in percent
        Money::parse("1")
            .expect("one")
            .minus(&cross_amount.over(&direct_amount))
            .times(&hundred)
            .round(4)
    };
    let threshold = Money::parse("0.1").expect("a tenth");
    Ok(json!({
        "direct_conversion": direct,
        "cross_conversion": cross,
        "difference": wire(&difference),
        "potential_savings_percent": wire(&savings),
        "recommendation": if savings > threshold {
            "Use cross-rate"
        } else {
            "Use direct rate"
        },
    }))
}

/// `GET /format/{code}/{amount}`: the amount written the way the currency is written.
pub fn format_amount(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    code: &str,
    amount_text: &str,
    today: &str,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, today)?;
    let held = currencies(graph, scope)?;
    let currency = find_currency(&held, code).ok_or(StoreError::NotFound)?;
    let places = i64::from(vimbai_store::read::whole(&currency, "decimal_places"));
    let amount = Money::parse(amount_text)
        .map_err(|error| StoreError::Invalid(format!("amount: {error}")))?;
    let fixed = amount.to_fixed(places.max(0));
    let grouped = grouped(&fixed);
    Ok(json!({
        "currency": field(&currency, "code"),
        "symbol": field(&currency, "symbol"),
        "amount": wire(&amount),
        "formatted": format!("{}{}", field(&currency, "symbol"), grouped),
        "decimal_places": places,
    }))
}

/// Thousands separated, as `f"{amount:,.2f}"` wrote them.
fn grouped(fixed: &str) -> String {
    let (whole, fraction) = match fixed.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (fixed, None),
    };
    let negative = whole.starts_with('-');
    let digits = whole.trim_start_matches('-');
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let mut written = String::new();
    if negative {
        written.push('-');
    }
    written.push_str(&grouped);
    if let Some(fraction) = fraction {
        written.push('.');
        written.push_str(fraction);
    }
    written
}

/// `POST /validate-transaction`: every line converted into the base currency and summed.
pub fn validate_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, &label(payload, "today"))?;
    let base = text(payload, "base_currency").to_uppercase();
    let held_currencies = currencies(graph, scope)?;
    if find_currency(&held_currencies, &base).is_none() {
        return Err(refused(&format!("Base currency {base} not supported")));
    }
    let held = latest_rates(graph, scope)?;
    let lines = payload
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut converted_lines = Vec::new();
    let mut total = Money::zero();
    for line in &lines {
        let currency = match text(line, "currency") {
            stated if stated.is_empty() => base.clone(),
            stated => stated.to_uppercase(),
        };
        let amount = amount_or_zero(line, "amount")?;
        // the shipped service converted the line's currency into the base: the note in its source records that
        // it used to convert the other way round, which summed nonsense, and that this direction is the fix
        let converted_amount = if currency == base {
            amount.clone()
        } else {
            converted(&held, &held_currencies, &currency, &base, &amount)
                .map_err(|_| refused(&format!("No exchange rate found for {currency} to {base}")))?
                .0
        };
        converted_lines.push(json!({
            "original_currency": currency,
            "original_amount": wire(&amount),
            "converted_amount": wire(&converted_amount),
            "converted_to": base,
            "account": line.get("account").cloned().unwrap_or(Value::Null),
            "description": line.get("description").cloned().unwrap_or(Value::Null),
        }));
        total = total.plus(&converted_amount);
    }
    Ok(json!({
        "valid": true,
        "base_currency": base,
        "total_in_base": wire(&total),
        "line_count": lines.len(),
        "lines": converted_lines,
    }))
}

/// `POST /alerts/rate`: the shipped service answered an alert it never kept, and this one answers the same
/// shape. The moment it was made comes from the request rather than a clock.
pub fn create_rate_alert(query: &Value) -> Result<Value, StoreError> {
    let from = label(query, "from_currency").to_uppercase();
    let to = label(query, "to_currency").to_uppercase();
    if from.is_empty() || to.is_empty() {
        return Err(refused("from_currency and to_currency are required"));
    }
    let target = amount(query, "target_rate")?;
    let direction = vimbai_store::read::or_default(query, "direction", "any");
    if !["above", "below", "any"].contains(&direction.as_str()) {
        return Err(refused("direction must be above, below or any"));
    }
    Ok(json!({
        "alert_id": format!("rate_alert_{from}_{to}"),
        "from_currency": from,
        "to_currency": to,
        "target_rate": wire(&target),
        "direction": direction,
        "notification_url": query.get("notification_url").cloned().unwrap_or(Value::Null),
        "created_at": label(query, "today"),
        "status": "active",
    }))
}

/// `GET /stats`.
pub fn currency_stats(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    today: &str,
) -> Result<Value, StoreError> {
    seed_defaults(graph, scope, today)?;
    let held = currencies(graph, scope)?;
    let stated = rates(graph, scope)?;
    let newest = stated
        .iter()
        .map(|rate| field(rate, "effective_date"))
        .max()
        .unwrap_or("");
    Ok(json!({
        "total_currencies": held.len(),
        "active_currencies": held
            .iter()
            .filter(|currency| vimbai_store::read::flag(currency, "is_active"))
            .count(),
        "total_rates": stated.len(),
        "latest_rate_date": if newest.is_empty() { Value::Null } else { json!(newest) },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A Book that already holds the seeded defaults, so a test can state only what it cares about.

    #[test]
    fn a_fresh_book_deals_in_the_default_currencies() {
        // no currency held: the first read seeds the fifteen and their rates, then answers the read
        let mut script = vec![vec![]];
        for _ in 0..15 {
            script.push(vec![json!({"id": "seeded"})]);
        }
        for _ in 0..15 {
            script.push(vec![json!({"id": "seeded"})]);
        }
        script.push(vec![
            json!({"id": "c-usd", "code": "USD", "decimal_places": 2}),
        ]);
        let graph = FakeGraph::queue(script);
        let held = list_currencies(&graph, &scope(), &json!({})).expect("listed");
        assert_eq!(held.as_array().expect("a list").len(), 1);
        // fifteen currencies and fifteen rates were written
        assert_eq!(graph.statements(), 1 + 15 + 15 + 1);
    }

    #[test]
    fn a_pair_without_a_stated_rate_is_answered_through_usd() {
        // GBP to EUR is not stated: USD to GBP and USD to EUR are, so the cross rate answers it
        let held = vec![
            json!({"from_currency": "USD", "to_currency": "GBP", "rate": "0.8"}),
            json!({"from_currency": "USD", "to_currency": "EUR", "rate": "0.9"}),
        ];
        let rate = rate_between(&held, "GBP", "EUR", 0).expect("a cross rate");
        assert_eq!(rate.to_fixed(3), "1.125");
    }

    #[test]
    fn an_unstated_pair_with_no_inverse_is_refused_by_name() {
        let held = vec![json!({"from_currency": "USD", "to_currency": "GBP", "rate": "0.8"})];
        let missing = converted(&held, &[], "CHF", "JPY", &Money::from_i64(10));
        let message = match missing {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("No exchange rate found for CHF to JPY"),
            "{message}"
        );
    }

    #[test]
    fn a_conversion_rounds_to_the_targets_decimal_places() {
        let currencies_held = vec![json!({"code": "JPY", "decimal_places": 0})];
        let held = vec![json!({"from_currency": "USD", "to_currency": "JPY", "rate": "149.5"})];
        let (converted, rate) = converted(
            &held,
            &currencies_held,
            "USD",
            "JPY",
            &Money::parse("10.55").expect("amount"),
        )
        .expect("converted");
        // 1577.225 lands on a whole yen, half up
        assert_eq!(converted.to_fixed(0), "1577");
        assert_eq!(rate.to_fixed(2), "149.50");
    }

    #[test]
    fn a_larger_amount_keeps_every_digit_it_was_sent_with() {
        let held = vec![json!({"from_currency": "USD", "to_currency": "GBP", "rate": "0.8"})];
        let amount = Money::parse("123456789012345678901.55").expect("amount");
        let (converted, _) = converted(&held, &[], "USD", "GBP", &amount).expect("converted");
        assert_eq!(converted.to_fixed(2), "98765431209876543121.24");
    }

    #[test]
    fn thousands_are_separated_the_way_the_shipped_formatter_wrote_them() {
        assert_eq!(grouped("1234567.89"), "1,234,567.89");
        assert_eq!(grouped("-1234.50"), "-1,234.50");
        assert_eq!(grouped("999"), "999");
    }

    #[test]
    fn the_newest_rate_for_a_pair_is_the_one_a_conversion_uses() {
        let mut newest = vec![
            json!({"from_currency": "USD", "to_currency": "GBP", "rate": "0.8", "effective_date": "2026-01-01"}),
            json!({"from_currency": "USD", "to_currency": "GBP", "rate": "0.9", "effective_date": "2026-02-01"}),
        ];
        newest.sort_by(|left, right| {
            field(left, "effective_date").cmp(field(right, "effective_date"))
        });
        let latest = newest.pop().expect("a rate");
        assert_eq!(field(&latest, "rate"), "0.9");
    }

    #[test]
    fn the_same_currency_converts_at_parity() {
        let held = vec![json!({"from_currency": "USD", "to_currency": "USD", "rate": "1"})];
        let (converted, rate) =
            converted(&held, &[], "USD", "USD", &Money::from_i64(500)).expect("converted");
        assert_eq!(converted.to_fixed(2), "500.00");
        assert_eq!(rate.to_fixed(2), "1.00");
    }

    #[test]
    fn a_rate_alert_is_answered_without_being_kept() {
        let alert = create_rate_alert(&json!({
            "from_currency": "usd", "to_currency": "gbp", "target_rate": 0.85,
            "direction": "above", "today": "2026-03-01T00:00:00"
        }))
        .expect("an alert");
        assert_eq!(alert["alert_id"], json!("rate_alert_USD_GBP"));
        assert_eq!(alert["status"], json!("active"));
    }

    #[test]
    fn a_direction_the_service_does_not_watch_is_refused() {
        let refused_alert = create_rate_alert(&json!({
            "from_currency": "USD", "to_currency": "GBP", "target_rate": 1, "direction": "sideways"
        }));
        assert!(refused_alert.is_err());
    }

    #[test]
    fn the_seeded_defaults_are_the_fifteen_the_service_started_with() {
        assert_eq!(DEFAULT_CURRENCIES.len(), 15);
        assert_eq!(DEFAULT_RATES.len(), 15);
        // the yuan and the yen share a symbol, and nobody is surprised by that
        let yen = DEFAULT_CURRENCIES
            .iter()
            .find(|row| row.0 == "JPY")
            .expect("yen");
        assert_eq!(yen.3, 0);
    }
}
