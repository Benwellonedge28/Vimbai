#![forbid(unsafe_code)]
//! audit-sampling-service: the sample an auditor draws from a population.
//!
//! A sample is answered and forgotten - the shipped service kept nothing - so this module holds no records and
//! needs no store.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the confidence factor is 1.96 at ninety-five per cent or above and 1.645 below it, and nothing else is
//!   consulted;
//! * an attribute sample sizes itself from the confidence factor alone and is capped at the population;
//! * a value sample uses the shipped formula, which puts the error rates in the numerator and the confidence
//!   factor and tolerance in the denominator (`pop * p(1-p) / (conf^2 * tolerance^2)`) - the inverse of the
//!   textbook calculation. The shipped answer is kept, because a caller comparing a drawn sample against a
//!   filed one would otherwise see every sample change size; and the sample is never smaller than thirty nor
//!   larger than the population, which is the clamp the shipped code applied;
//! * a stratified draw splits by risk band (seventy and above high, forty and above medium, the rest low), takes
//!   half for high, thirty-five per cent for medium and fifteen per cent for low, but never fewer items than the
//!   band actually holds - so a small band is taken whole and the sample can exceed the nominal size;
//! * a stratified draw is NOT random: it takes the band in order. A plain draw is random from a stated seed, and
//!   the same seed draws the same items.
//! * the interval is the population over the sample size (one when the sample is empty) and the margin is half
//!   the tolerable error rate;
//! * the conclusion multiplies the stated confidence by a hundred, and the filed confidence is already a
//!   percentage, so a ninety-five per cent sample states "9500% confidence". That is what the shipped service
//!   answered - it is pinned by a test rather than quietly corrected, because a caller reading this field today
//!   reads that number.

use serde_json::{json, Value};

/// The confidence factor a stated confidence earns.
pub fn confidence_factor(confidence_level: f64) -> f64 {
    if confidence_level >= 95.0 {
        1.96
    } else {
        1.645
    }
}

/// The sample size a method and a population earn.
pub fn sample_size(
    population: usize,
    expected: f64,
    tolerable: f64,
    confidence: f64,
    method: &str,
) -> f64 {
    let factor = confidence_factor(confidence);
    let pop = population as f64;
    if method == "attribute" {
        // the shipped attribute size consulted the confidence factor and the tolerance only
        let size = (factor * factor * 0.5 * 0.5) / (tolerable * tolerable);
        return size.min(pop);
    }
    // the shipped formula: the error rates in the numerator, the factor and tolerance in the denominator
    let size = pop * (expected * (1.0 - expected)) / (factor * factor * tolerable * tolerable);
    size.max(30.0).min(pop)
}

/// The band a risk score sits in.
pub fn band(risk: u32) -> &'static str {
    if risk >= 70 {
        "high"
    } else if risk >= 40 {
        "medium"
    } else {
        "low"
    }
}

fn amount_of(item: &Value) -> Value {
    item.get("amount").cloned().unwrap_or_else(|| json!(0.0))
}

fn sample_item(item: &Value) -> Value {
    json!({
        "item_id": item.get("item_id").cloned().unwrap_or_else(|| json!("")),
        // the shipped answer called the date `transaction_date` while it read `date` from the population
        "transaction_date": item.get("date").cloned().unwrap_or_else(|| json!("")),
        "amount": amount_of(item),
        "description": item.get("description").cloned().unwrap_or_else(|| json!("")),
        "risk_score": item.get("risk_score").cloned().unwrap_or_else(|| json!(50)),
    })
}

/// A small generator standing in for the shipped `random.sample`: the same seed draws the same items.
struct Draw {
    state: u64,
}

impl Draw {
    fn new(seed: Option<i64>) -> Self {
        let seed = seed.unwrap_or(42);
        Self {
            state: (seed as u64)
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407),
        }
    }

    fn next(&mut self) -> u64 {
        // xorshift64*, chosen because it is short enough to read and does not need a crate
        let mut held = self.state;
        held ^= held >> 12;
        held ^= held << 25;
        held ^= held >> 27;
        self.state = held;
        held.wrapping_mul(2_685_821_657_736_338_717)
    }

    /// `count` distinct positions out of `population`, in the order they were drawn.
    fn draw(&mut self, population: usize, count: usize) -> Vec<usize> {
        let mut picked: Vec<usize> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        while picked.len() < count.min(population) {
            let position = (self.next() % population as u64) as usize;
            seen.insert(position);
            picked = seen.iter().copied().collect();
        }
        picked
    }
}

/// `POST /sample`: draw the sample.
pub fn draw_sample(payload: &Value) -> Result<Value, String> {
    let population = match payload.get("population") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let parameters = payload
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let number = |key: &str, fallback: f64| {
        parameters
            .get(key)
            .and_then(|held| match held {
                Value::Number(number) => number.as_f64(),
                Value::String(digits) => digits.trim().parse().ok(),
                _ => None,
            })
            .unwrap_or(fallback)
    };
    let expected = number("expected_error_rate", 0.0);
    let tolerable = number("tolerable_error_rate", 0.0);
    let confidence = number("confidence_level", 95.0);
    let method = parameters
        .get("sampling_method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    if tolerable <= 0.0 {
        // the shipped code divided by the tolerance; a zero tolerance has no sample to give
        return Err("tolerable_error_rate must be greater than zero".to_string());
    }

    let size = sample_size(population.len(), expected, tolerable, confidence, &method);
    let stratify = payload
        .get("stratification")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let chosen: Vec<Value> = if stratify {
        // a stratified draw takes each band in order rather than at random
        let mut taken: Vec<Value> = Vec::new();
        for (wanted, share) in [("high", 0.5), ("medium", 0.35), ("low", 0.15)] {
            let band_items: Vec<&Value> = population
                .iter()
                .filter(|item| {
                    let risk = item.get("risk_score").and_then(Value::as_u64).unwrap_or(50) as u32;
                    band(risk) == wanted
                })
                .collect();
            // never fewer items than the band holds, which is the max the shipped code took
            let count = ((size * share) as usize).max(band_items.len());
            for item in band_items.into_iter().take(count) {
                taken.push(sample_item(item));
            }
        }
        taken
    } else {
        let seed = payload.get("random_seed").and_then(Value::as_i64);
        let mut draw = Draw::new(seed);
        let positions = draw.draw(population.len(), size as usize);
        positions
            .into_iter()
            .filter_map(|position| population.get(position))
            .map(sample_item)
            .collect()
    };

    let interval = if chosen.is_empty() {
        1.0
    } else {
        population.len() as f64 / chosen.len() as f64
    };
    let margin = tolerable * 0.5;
    let counted = |wanted: &str| {
        chosen
            .iter()
            .filter(|item| {
                let risk = item.get("risk_score").and_then(Value::as_u64).unwrap_or(50) as u32;
                band(risk) == wanted
            })
            .count()
    };
    Ok(json!({
        "audit_id": payload.get("audit_id").cloned().unwrap_or_else(|| json!("")),
        "account_id": payload.get("account_id").cloned().unwrap_or_else(|| json!("")),
        "sample_size": chosen.len(),
        "sample": chosen,
        "stratification_levels": {
            "high": counted("high"),
            "medium": counted("medium"),
            "low": counted("low"),
        },
        "sampling_interval": round(interval, 2),
        "confidence_level": confidence,
        "margin_of_error": round(margin, 4),
        "statistical_conclusion": format!(
            "Sample of {} items provides {}% confidence",
            chosen.len(),
            (confidence * 100.0) as i64
        ),
    }))
}

fn round(value: f64, places: i32) -> f64 {
    let factor = 10f64.powi(places);
    (value * factor).round() / factor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn population(count: usize) -> Vec<Value> {
        (0..count)
            .map(|index| {
                json!({
                    "item_id": format!("T-{index:03}"),
                    "date": "2026-01-15",
                    "amount": 100.0 + index as f64,
                    "description": format!("Sale {index}"),
                    "risk_score": if index % 3 == 0 { 80 } else if index % 3 == 1 { 50 } else { 20 },
                })
            })
            .collect()
    }

    fn request(count: usize, method: &str, stratify: bool) -> Value {
        json!({
            "audit_id": "A-1",
            "account_id": "ACC-1",
            "population": population(count),
            "parameters": {
                "population_size": count,
                "expected_error_rate": 0.05,
                "tolerable_error_rate": 0.05,
                "confidence_level": 95.0,
                "sampling_method": method,
            },
            "stratification": stratify,
            "random_seed": 42,
        })
    }

    #[test]
    fn the_confidence_factor_is_one_of_two_numbers() {
        assert_eq!(confidence_factor(95.0), 1.96);
        assert_eq!(confidence_factor(99.0), 1.96);
        assert_eq!(confidence_factor(90.0), 1.645);
    }

    #[test]
    fn an_attribute_sample_sizes_itself_from_the_factor_and_is_capped_at_the_population() {
        // 1.96^2 * 0.25 / 0.05^2 is about 384, well above a hundred
        assert_eq!(sample_size(100, 0.05, 0.05, 95.0, "attribute"), 100.0);
        let big = sample_size(10_000, 0.05, 0.05, 95.0, "attribute");
        assert!((big - 384.16).abs() < 0.01, "{big}");
    }

    #[test]
    fn a_value_sample_keeps_the_shipped_formula_and_its_clamp() {
        // the shipped formula usually asks for more than the population, so the sample saturates at it
        assert_eq!(sample_size(1_000, 0.05, 0.05, 95.0, "monetary"), 1_000.0);
        assert_eq!(sample_size(100_000, 0.5, 0.05, 95.0, "monetary"), 100_000.0);
        // and where it asks for almost nothing, the thirty item floor is what it gets
        assert_eq!(sample_size(1_000, 0.0001, 0.05, 95.0, "monetary"), 30.0);
    }

    #[test]
    fn an_attribute_draw_counts_the_items_it_took() {
        let drawn = draw_sample(&request(50, "attribute", false)).expect("a sample");
        // the attribute size is capped at the population, so every item is drawn
        assert_eq!(drawn["sample_size"], json!(50));
        assert_eq!(drawn["sampling_interval"], json!(1.0));
        assert_eq!(drawn["margin_of_error"], json!(0.025));
        // the filed confidence is a percentage and the shipped conclusion multiplied it by a hundred again
        assert_eq!(
            drawn["statistical_conclusion"],
            json!("Sample of 50 items provides 9500% confidence")
        );
        // the shipped answer called the date transaction_date while reading date
        assert_eq!(drawn["sample"][0]["transaction_date"], json!("2026-01-15"));
    }

    #[test]
    fn a_plain_draw_is_random_but_the_same_seed_draws_the_same_items() {
        // a small expected error rate is what leaves a value sample smaller than its population
        let mut small = request(200, "monetary", false);
        small["parameters"]["expected_error_rate"] = json!(0.0001);
        let draw_once = draw_sample(&small).expect("a sample");
        let drawn_twice = draw_sample(&small).expect("a sample");
        assert_eq!(draw_once["sample"], drawn_twice["sample"]);
        assert_eq!(draw_once["sample_size"], json!(30));

        let mut with_another = small.clone();
        with_another["random_seed"] = json!(7);
        let other = draw_sample(&with_another).expect("a sample");
        assert_ne!(draw_once["sample"], other["sample"]);
    }

    #[test]
    fn a_stratified_draw_takes_whole_bands_and_can_beat_its_nominal_size() {
        let drawn = draw_sample(&request(300, "monetary", true)).expect("a sample");
        // a hundred in each band, with a thirty item nominal size: half of thirty is fifteen, which is fewer
        // than the band holds, so each band is taken whole
        assert_eq!(drawn["sample_size"], json!(300));
        assert_eq!(drawn["stratification_levels"]["high"], json!(100));
        assert_eq!(drawn["stratification_levels"]["medium"], json!(100));
        assert_eq!(drawn["stratification_levels"]["low"], json!(100));
        assert_eq!(drawn["sampling_interval"], json!(1.0));
    }

    #[test]
    fn a_population_with_no_tolerance_is_refused_rather_than_divided_by_zero() {
        let mut request = request(10, "attribute", false);
        request["parameters"]["tolerable_error_rate"] = json!(0.0);
        assert!(draw_sample(&request).is_err());
    }

    #[test]
    fn an_empty_population_answers_a_one_item_interval_and_no_sample() {
        let drawn = draw_sample(&request(0, "monetary", false)).expect("a sample");
        assert_eq!(drawn["sample_size"], json!(0));
        assert_eq!(drawn["sampling_interval"], json!(1.0));
        assert_eq!(drawn["sample"], json!([]));
    }

    #[test]
    fn the_bands_are_seventy_and_forty() {
        assert_eq!(band(70), "high");
        assert_eq!(band(69), "medium");
        assert_eq!(band(40), "medium");
        assert_eq!(band(39), "low");
    }
}
