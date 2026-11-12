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

#![forbid(unsafe_code)]

use serde_json::Value;

fn close(expected: f64, actual: f64) -> bool {
    if expected == actual {
        return true;
    }
    let scale = expected.abs().max(actual.abs()).max(1.0);
    (expected - actual).abs() <= 1e-9 * scale
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
