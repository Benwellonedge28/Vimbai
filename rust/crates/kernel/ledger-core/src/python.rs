//! PyO3 bindings: exposes the ledger kernel to Vimbai's Python services
//! as `vimbai_ledger_core`.
//!
//! Built with the `python` feature:
//! `maturin develop --release` (dev) or `maturin build` (wheel).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::double_entry::{validate_lines, JournalLine};
use crate::hash_chain::{chain_state_from_digests, compute_entry_hash, EntryDigest, GENESIS_HASH};
use crate::reversal::{check_reversal, flatten_lines, AccountMovement};

fn pyerr<E: std::fmt::Display>(e: E) -> PyErr {
    PyValueError::new_err(format!("ledger_core: {e}"))
}

#[pyfunction]
fn validate_double_entry(lines: Vec<(f64, f64)>) -> PyResult<bool> {
    let lines: Vec<JournalLine> = lines
        .into_iter()
        .map(|(d, c)| JournalLine::new(d, c))
        .collect();
    let report = validate_lines(&lines);
    if report.valid {
        Ok(true)
    } else {
        Err(pyerr(
            serde_json::to_string(&report.errors).unwrap_or_else(|_| "invalid entry".into()),
        ))
    }
}

#[pyfunction]
fn validate_double_entry_report(lines: Vec<(f64, f64)>) -> PyResult<String> {
    let lines: Vec<JournalLine> = lines
        .into_iter()
        .map(|(d, c)| JournalLine::new(d, c))
        .collect();
    let report = validate_lines(&lines);
    serde_json::to_string(&report).map_err(pyerr)
}

#[pyfunction(signature = (payload, prev_hash=None))]
fn hash_entry(payload: &Bound<'_, PyDict>, prev_hash: Option<String>) -> PyResult<String> {
    let json_value = dict_to_json(payload)?;
    let prev = prev_hash.unwrap_or_else(|| GENESIS_HASH.to_string());
    compute_entry_hash(&prev, &json_value).map_err(pyerr)
}

#[pyfunction]
fn genesis_hash() -> &'static str {
    GENESIS_HASH
}

/// Stamps a payload against the genesis predecessor (first entry of a
/// chain, or an independent digest).
#[pyfunction(signature = (payload))]
fn stamp_genesis(payload: &Bound<'_, PyDict>) -> PyResult<String> {
    let json_value = dict_to_json(payload)?;
    compute_entry_hash(GENESIS_HASH, &json_value).map_err(pyerr)
}

#[pyfunction]
fn verify_chain(entries: Vec<Bound<'_, PyDict>>) -> PyResult<String> {
    let mut digests = Vec::with_capacity(entries.len());
    for e in entries {
        let json_value = dict_to_json(&e)?;
        let obj = match json_value {
            serde_json::Value::Object(map) => map,
            _ => {
                return Err(PyValueError::new_err(
                    "ledger_core: entry payload must be a dict",
                ))
            }
        };
        let entry_id = obj
            .get("entry_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PyValueError::new_err("ledger_core: entry requires entry_id"))?
            .to_string();
        let prev_hash = obj
            .get("prev_hash")
            .and_then(|v| v.as_str())
            .unwrap_or(GENESIS_HASH)
            .to_string();
        let stored_hash = obj
            .get("stored_hash")
            .and_then(|v| v.as_str())
            .map(String::from);
        let payload = match obj.get("payload") {
            Some(serde_json::Value::Object(p)) => p.clone(),
            Some(_) => return Err(PyValueError::new_err("ledger_core: payload must be a dict")),
            None => {
                return Err(PyValueError::new_err(
                    "ledger_core: entry requires a nested payload dict",
                ))
            }
        };
        digests.push(EntryDigest {
            entry_id,
            payload,
            prev_hash,
            stored_hash,
        });
    }
    let report = chain_state_from_digests(&digests);
    serde_json::to_string(&report).map_err(pyerr)
}

#[pyfunction]
fn check_reversal_mirror(
    original: Vec<Bound<'_, PyDict>>,
    reversal: Vec<Bound<'_, PyDict>>,
) -> PyResult<bool> {
    let to_movements = |v: &[Bound<'_, PyDict>]| -> PyResult<Vec<AccountMovement>> {
        v.iter()
            .map(|d| {
                Ok(AccountMovement {
                    account: d.get_item("account")?.unwrap().extract()?,
                    debit: d.get_item("debit")?.unwrap().extract()?,
                    credit: d.get_item("credit")?.unwrap().extract()?,
                })
            })
            .collect()
    };
    let r = check_reversal(&to_movements(&original)?, &to_movements(&reversal)?);
    if r.valid {
        Ok(true)
    } else {
        Err(pyerr(
            serde_json::to_string(&r.errors).unwrap_or_else(|_| "invalid reversal".into()),
        ))
    }
}

#[pyfunction]
fn flatten_lines_by_account(lines: Vec<(f64, f64)>, accounts: Vec<String>) -> PyResult<String> {
    let lines: Vec<JournalLine> = lines
        .into_iter()
        .map(|(d, c)| JournalLine::new(d, c))
        .collect();
    let flat = flatten_lines(&lines, &accounts);
    serde_json::to_string(&flat).map_err(pyerr)
}

fn dict_to_json(dict: &Bound<'_, PyDict>) -> PyResult<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        let value = py_to_json(&v)?;
        map.insert(key, value);
    }
    Ok(serde_json::Value::Object(map))
}

fn py_to_json(v: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    use pyo3::types::PyAnyMethods;
    if v.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = v.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = v.extract::<i64>() {
        Ok(serde_json::Value::from(i))
    } else if let Ok(f) = v.extract::<f64>() {
        if f.is_finite() {
            Ok(serde_json::Value::from(f))
        } else {
            Ok(serde_json::Value::Null)
        }
    } else if let Ok(s) = v.extract::<String>() {
        Ok(serde_json::Value::from(s))
    } else if let Ok(list) = v.downcast::<pyo3::types::PyList>() {
        let mut arr = Vec::with_capacity(list.len());
        for item in list.iter() {
            arr.push(py_to_json(&item)?);
        }
        Ok(serde_json::Value::Array(arr))
    } else if let Ok(d) = v.downcast::<pyo3::types::PyDict>() {
        dict_to_json(d)
    } else {
        Err(PyValueError::new_err(format!(
            "ledger_core: unsupported payload type: {}",
            v.get_type().name()?
        )))
    }
}

#[pymodule]
fn vimbai_ledger_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(validate_double_entry, m)?)?;
    m.add_function(wrap_pyfunction!(validate_double_entry_report, m)?)?;
    m.add_function(wrap_pyfunction!(hash_entry, m)?)?;
    m.add_function(wrap_pyfunction!(genesis_hash, m)?)?;
    m.add_function(wrap_pyfunction!(stamp_genesis, m)?)?;
    m.add_function(wrap_pyfunction!(verify_chain, m)?)?;
    m.add_function(wrap_pyfunction!(check_reversal_mirror, m)?)?;
    m.add_function(wrap_pyfunction!(flatten_lines_by_account, m)?)?;
    Ok(())
}
