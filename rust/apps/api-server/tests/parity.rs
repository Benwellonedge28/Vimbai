//! Parity: the Rust calculator against the answer the Python service gave.
//!
//! Each file under `tests/parity/` is a capture: the request that was posted to a Python service
//! and the JSON it answered with. The capture is the evidence, kept after the service is deleted,
//! so a later change to a calculator that shifts a figure is caught rather than discovered in
//! production.
//!
//! Floats are compared with a tolerance rather than exactly, because Python and Rust agree to
//! about fifteen significant digits and no further; a figure that moves by more than that is a
//! real difference, not a printing one.
//!
//! An amount may travel as text rather than as a JSON number, because text is the only shape that
//! keeps every digit of a figure past a double's reach. A text amount is still read and compared
//! against the captured figure, so moving a field to text never turns its check off.

#![forbid(unsafe_code)]

use serde_json::Value;

fn close(expected: f64, actual: f64) -> bool {
    if expected == actual {
        return true;
    }
    let scale = expected.abs().max(actual.abs()).max(1.0);
    (expected - actual).abs() <= 1e-9 * scale
}

/// A figure that arrived as text, read for comparison. `None` when the text is not a number at all,
/// in which case the two answers genuinely differ.
fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn compare(
    path: &str,
    expected: &Value,
    actual: &Value,
    ignore: &[String],
    problems: &mut Vec<String>,
) {
    if ignore.iter().any(|field| field == path) {
        return;
    }
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => {
            let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) else {
                problems.push(format!("{path}: not comparable as floats"));
                return;
            };
            if !close(a, b) {
                problems.push(format!("{path}: python {a} != rust {b}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if ignore.iter().any(|field| field == &child) {
                    continue;
                }
                match b.get(key) {
                    Some(other) => compare(&child, value, other, ignore, problems),
                    None => problems.push(format!("{child}: missing from the Rust answer")),
                }
            }
            for key in b.keys() {
                if !a.contains_key(key) {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    if ignore.iter().any(|field| field == &child) {
                        continue;
                    }
                    problems.push(format!("{child}: present only in the Rust answer"));
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                problems.push(format!(
                    "{path}: python {} entries != rust {}",
                    a.len(),
                    b.len()
                ));
                return;
            }
            for (index, (left, right)) in a.iter().zip(b.iter()).enumerate() {
                compare(&format!("{path}[{index}]"), left, right, ignore, problems);
            }
        }
        (a, b) => {
            if let (Some(expected_number), Some(actual_number)) = (as_number(a), as_number(b)) {
                if !close(expected_number, actual_number) {
                    problems.push(format!(
                        "{path}: python {expected_number} != rust {actual_number}"
                    ));
                }
                return;
            }
            if a != b {
                problems.push(format!("{path}: python {a} != rust {b}"));
            }
        }
    }
}

#[test]
fn every_capture_is_reproduced() {
    let calculators = vimbai_api::calculators();
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity");
    let mut captures = 0;
    let mut problems = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(&directory)
        .expect("the parity directory is part of the crate")
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort();

    for path in entries {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let capture: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let route = capture["route"]
            .as_str()
            .expect("a capture names its route");
        let Some(calculator) = calculators.get(route) else {
            problems.push(format!("{name}: no calculator registered as {route}"));
            continue;
        };
        let ignore: Vec<String> = capture["ignore"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        match calculator(&capture["request"]) {
            Ok(answer) => compare("", &capture["response"], &answer, &ignore, &mut problems),
            Err(error) => problems.push(format!("{name}: calculator refused the request: {error}")),
        }
        captures += 1;
    }

    assert!(
        captures >= 4,
        "expected the captured services, found {captures}"
    );
    assert!(
        problems.is_empty(),
        "parity broken:\n{}",
        problems.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::as_number;
    use serde_json::json;

    #[test]
    fn an_amount_as_text_is_still_a_figure_to_compare() {
        // Text is how a figure past a double's reach travels, so it must not slip past the comparison.
        let read = |value| as_number(&value).map(|number| (number * 1000.0).round() as i64);
        assert_eq!(read(json!("1000000.00")), Some(1_000_000_000));
        assert_eq!(read(json!(-12.5)), Some(-12_500));
        assert_eq!(as_number(&json!("not an amount")), None);
        assert_eq!(as_number(&json!(null)), None);
    }
}
