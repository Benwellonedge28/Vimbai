//! Currency master data and conversion: direct, inverse and USD-cross
//! rates, rounding to a currency's decimal places, triangulation,
//! formatting and multi-currency transaction validation.
use serde::{Deserialize, Serialize};

/// One currency's master record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Currency {
    pub code: String,
    pub name: String,
    pub symbol: String,
    pub decimal_places: u32,
    pub is_active: bool,
}

/// One stored exchange rate. The date is carried but never used when
/// selecting a rate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub effective_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurrencyError {
    /// No direct, inverse or USD-cross rate exists for the pair.
    NoRate { from: String, to: String },
    /// The base currency of a transaction is not in the currency master.
    UnsupportedBase { code: String },
    /// A conversion could not be rounded because the rate is not finite.
    NonFiniteRate { from: String, to: String },
}

impl std::fmt::Display for CurrencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CurrencyError::NoRate { from, to } => {
                write!(f, "No exchange rate found for {from} to {to}")
            }
            CurrencyError::UnsupportedBase { code } => {
                write!(f, "Base currency {code} not supported")
            }
            CurrencyError::NonFiniteRate { from, to } => {
                write!(f, "exchange rate for {from} to {to} is not a finite number")
            }
        }
    }
}

impl std::error::Error for CurrencyError {}

/// Round half away from zero at `dp` places, matching Python's
/// `Decimal(str(x)).quantize(..., ROUND_HALF_UP)` on the shortest
/// round-trip representation of the value.
pub fn round_half_up(value: f64, dp: u32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let scale = 10f64.powi(dp as i32);
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let scaled = value.abs() * scale;
    let floored = scaled.floor();
    let fraction = scaled - floored;
    // The slack absorbs binary representation error in ties like 2.675.
    let rounded = if fraction > 0.5 - 1e-9 {
        floored + 1.0
    } else {
        floored
    };
    sign * rounded / scale
}

/// Round half to even, matching Python's built-in `round(value, dp)`.
pub fn round_half_even(value: f64, dp: u32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    // Rust's float formatting rounds the EXACT binary value with
    // ties-to-even, which is what Python's round() does: 2.675 is really
    // 2.67499999... in binary, so it goes DOWN to 2.67.
    format!("{:.*}", dp as usize, value)
        .parse::<f64>()
        .unwrap_or(value)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversion {
    pub from_currency: String,
    pub to_currency: String,
    pub original_amount: f64,
    pub converted_amount: f64,
    pub rate_used: f64,
    pub rate_date: String,
    pub rounding_mode: String,
}

/// The caller's rate book plus currency master data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateBook {
    pub rates: Vec<ExchangeRate>,
    pub currencies: Vec<Currency>,
}

impl RateBook {
    pub fn new(rates: Vec<ExchangeRate>, currencies: Vec<Currency>) -> Self {
        Self { rates, currencies }
    }

    fn lookup(&self, from: &str, to: &str) -> Option<f64> {
        // Last record for a pair wins: the book is not date-aware.
        self.rates
            .iter()
            .rev()
            .find(|r| r.from_currency == from && r.to_currency == to)
            .map(|r| r.rate)
    }

    /// Direct rate, then the inverse of the opposite pair, then a cross
    /// through USD. The date argument exists in the source but is ignored.
    pub fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        if let Some(rate) = self.lookup(from, to) {
            return Some(rate);
        }
        if let Some(inverse) = self.lookup(to, from) {
            return Some(1.0 / inverse);
        }
        if from != "USD" && to != "USD" {
            let from_usd = self.get_rate("USD", from)?;
            let usd_to = self.get_rate("USD", to)?;
            return Some(usd_to / from_usd);
        }
        None
    }

    fn decimal_places(&self, code: &str) -> u32 {
        self.currencies
            .iter()
            .find(|c| c.code == code)
            .map(|c| c.decimal_places)
            .unwrap_or(2)
    }

    fn symbol(&self, code: &str) -> String {
        self.currencies
            .iter()
            .find(|c| c.code == code)
            .map(|c| c.symbol.clone())
            .unwrap_or_default()
    }

    /// `/convert`. The amount is rounded half up to the TARGET currency's
    /// decimal places; the rounding_mode argument is recorded but both of
    /// its branches round half up.
    pub fn convert(
        &self,
        from: &str,
        to: &str,
        amount: f64,
        rate_date: &str,
    ) -> Result<Conversion, CurrencyError> {
        let rate = self
            .get_rate(from, to)
            .ok_or_else(|| CurrencyError::NoRate {
                from: from.to_string(),
                to: to.to_string(),
            })?;
        if !rate.is_finite() {
            return Err(CurrencyError::NonFiniteRate {
                from: from.to_string(),
                to: to.to_string(),
            });
        }
        let converted = round_half_up(amount * rate, self.decimal_places(to));

        Ok(Conversion {
            from_currency: from.to_string(),
            to_currency: to.to_string(),
            original_amount: amount,
            converted_amount: converted,
            rate_used: rate,
            rate_date: rate_date.to_string(),
            rounding_mode: "HALF_UP".to_string(),
        })
    }

    /// `/format`.
    // `% 3` rather than `usize::is_multiple_of`: the wheel is built in the
    // maturin image, whose Rust is older than that method.
    #[allow(clippy::manual_is_multiple_of)]
    pub fn format_amount(&self, code: &str, amount: f64) -> String {
        let dp = self.decimal_places(code) as usize;
        let negative = amount < 0.0;
        let body = format!("{:.*}", dp, amount.abs());
        let (whole, fraction) = match body.split_once('.') {
            Some((w, f)) => (w.to_string(), f.to_string()),
            None => (body.clone(), String::new()),
        };

        // Insert thousand separators into the integer part.
        let mut grouped = String::new();
        let digits: Vec<char> = whole.chars().collect();
        for (i, ch) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(*ch);
        }
        let formatted = if fraction.is_empty() {
            grouped
        } else {
            format!("{grouped}.{fraction}")
        };

        format!(
            "{}{}{}",
            self.symbol(code),
            if negative { "-" } else { "" },
            formatted
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Triangulation {
    pub direct_conversion: Option<Conversion>,
    pub cross_conversion: Option<Conversion>,
    pub difference: Option<f64>,
    pub potential_savings_percent: Option<f64>,
    pub recommendation: String,
}

/// `/triangulate`. The cross leg is converted from the ALREADY ROUNDED
/// intermediate amount, as in the source, and the recommendation switches
/// at a tenth of a per cent.
pub fn triangulate(
    book: &RateBook,
    from: &str,
    to: &str,
    amount: f64,
) -> Result<Triangulation, CurrencyError> {
    let direct = book.convert(from, to, amount, "")?;

    if from != "USD" && to != "USD" {
        let mid = book.convert(from, "USD", amount, "")?;
        let cross = book.convert("USD", to, mid.converted_amount, "")?;
        let difference = (direct.converted_amount - cross.converted_amount).abs();
        let savings = if direct.converted_amount > 0.0 {
            (1.0 - cross.converted_amount / direct.converted_amount) * 100.0
        } else {
            0.0
        };
        return Ok(Triangulation {
            difference: Some(difference),
            potential_savings_percent: Some(savings),
            recommendation: if savings > 0.1 {
                "Use cross-rate".to_string()
            } else {
                "Use direct rate".to_string()
            },
            direct_conversion: Some(direct),
            cross_conversion: Some(cross),
        });
    }

    Ok(Triangulation {
        direct_conversion: Some(direct),
        cross_conversion: None,
        difference: None,
        potential_savings_percent: None,
        recommendation: "No triangulation needed (same base currency)".to_string(),
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionLine {
    pub currency: String,
    pub amount: f64,
    pub account: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConvertedLine {
    pub original_currency: String,
    pub original_amount: f64,
    pub converted_amount: f64,
    pub converted_to: String,
    pub account: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidatedTransaction {
    pub valid: bool,
    pub base_currency: String,
    pub total_in_base: f64,
    pub line_count: usize,
    pub lines: Vec<ConvertedLine>,
}

/// `/validate-transaction`. Each line is converted INTO the base currency
/// (the direction was fixed in the Python source), rounded at that currency's
/// places, then summed. The base currency must exist in the master data.
pub fn validate_multi_currency_transaction(
    book: &RateBook,
    base_currency: &str,
    lines: &[TransactionLine],
) -> Result<ValidatedTransaction, CurrencyError> {
    let base = base_currency.to_uppercase();
    if !book.currencies.iter().any(|c| c.code == base) {
        return Err(CurrencyError::UnsupportedBase { code: base });
    }

    let mut converted_lines = Vec::new();
    let mut total = 0.0;

    for line in lines {
        let currency = if line.currency.is_empty() {
            base.clone()
        } else {
            line.currency.to_uppercase()
        };
        let converted_amount = if currency != base {
            book.convert(&currency, &base, line.amount, "")?
                .converted_amount
        } else {
            line.amount
        };
        total += converted_amount;
        converted_lines.push(ConvertedLine {
            original_currency: currency,
            original_amount: line.amount,
            converted_amount,
            converted_to: base.clone(),
            account: line.account.clone(),
            description: line.description.clone(),
        });
    }

    Ok(ValidatedTransaction {
        valid: true,
        base_currency: base,
        total_in_base: total,
        line_count: lines.len(),
        lines: converted_lines,
    })
}
