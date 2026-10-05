//! Immutable hash chain: tamper-evidence for the shared ledger.
//!
//! Every journal entry stores `entry_hash = SHA256(prev_hash || payload)`
//! where `payload` is a canonical JSON dump of the entry's business fields
//! (keys sorted). Because ledger entries are immutable (corrections are
//! reversing entries only), the chain over `created_at` order is stable and
//! any retroactive change breaks every later stamp.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The all-zero stamp that seeds the first entry of a chain.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// A chain verification failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ChainError {
    /// A stored hash does not match recomputation over its payload.
    HashMismatch {
        entry_id: String,
        expected: String,
        actual: String,
    },
    /// An entry's stored predecessor stamp does not match the previous
    /// entry's stored stamp.
    BrokenLink {
        entry_id: String,
        prev_in_chain: String,
        stored_prev: String,
    },
    /// A payload field set could not be canonicalized.
    InvalidPayload { entry_id: String, reason: String },
}

/// The digest inputs for one entry, as persisted by the ledger writer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryDigest {
    pub entry_id: String,
    /// The business payload: string keys -> JSON-serializable values.
    /// Canonicalized internally (sorted keys, no whitespace).
    pub payload: serde_json::Map<String, serde_json::Value>,
    /// Stamp of the predecessor this entry chains from.
    pub prev_hash: String,
    /// Stored stamp for this entry (empty/None when the caller wants the
    /// computed stamp without verification).
    pub stored_hash: Option<String>,
}

/// Stamps a payload against a predecessor stamp.
///
/// `payload` must be a JSON object; keys are sorted and the payload is
/// serialized compactly, so callers never depend on field order.
pub fn compute_entry_hash(prev_hash: &str, payload: &serde_json::Value) -> Result<String, String> {
    let canonical = canonicalize(payload)?;
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(canonical.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

/// Serialize a JSON value canonically: recursive key sort, no whitespace.
fn canonicalize(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut parts = Vec::with_capacity(keys.len());
            for k in keys {
                let v = map.get(k).ok_or_else(|| "missing key".to_string())?;
                let v = canonicalize(v)?;
                parts.push(format!(
                    "{}:{}",
                    serde_json::to_string(k).map_err(|e| e.to_string())?,
                    v
                ));
            }
            Ok(format!("{{{}}}", parts.join(",")))
        }
        serde_json::Value::Array(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(canonicalize(item)?);
            }
            Ok(format!("[{}]", parts.join(",")))
        }
        _ => serde_json::to_string(value).map_err(|e| e.to_string()),
    }
}

/// Result of walking a chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainReport {
    pub valid: bool,
    pub entries_checked: usize,
    pub head_hash: String,
    pub errors: Vec<ChainError>,
}

/// Walks a full chain in ledger order and verifies every stamp.
pub fn chain_state_from_digests(digests: &[EntryDigest]) -> ChainReport {
    let mut errors = Vec::new();
    let mut prev = GENESIS_HASH.to_string();
    let mut head = GENESIS_HASH.to_string();

    for d in digests {
        if d.prev_hash != prev {
            errors.push(ChainError::BrokenLink {
                entry_id: d.entry_id.clone(),
                prev_in_chain: prev.clone(),
                stored_prev: d.prev_hash.clone(),
            });
        }
        let payload = serde_json::Value::Object(d.payload.clone());
        let computed = match compute_entry_hash(&d.prev_hash, &payload) {
            Ok(h) => h,
            Err(reason) => {
                errors.push(ChainError::InvalidPayload {
                    entry_id: d.entry_id.clone(),
                    reason,
                });
                // Cannot continue a chain past an unstamped entry.
                break;
            }
        };
        if let Some(stored) = &d.stored_hash {
            if stored != &computed {
                errors.push(ChainError::HashMismatch {
                    entry_id: d.entry_id.clone(),
                    expected: computed.clone(),
                    actual: stored.clone(),
                });
            }
        }
        prev = computed.clone();
        head = computed;
    }

    ChainReport {
        valid: errors.is_empty(),
        entries_checked: digests.len(),
        head_hash: head,
        errors,
    }
}

/// Convenience alias matching the crate docs.
pub type ChainState = ChainReport;

/// Seeds the first stamp: convenience for writers starting a new chain.
pub fn genesis_stamp(payload: &serde_json::Value) -> Result<String, String> {
    compute_entry_hash(GENESIS_HASH, payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> serde_json::Value {
        json!({
            "description": "Cash sale",
            "entry_date": "2026-10-05T00:00:00Z",
            "lines": [
                {"account": "1010", "debit": 100.0, "credit": 0.0},
                {"account": "4000", "debit": 0.0, "credit": 100.0}
            ],
            "reference_number": "REF-1",
            "status": "posted"
        })
    }

    #[test]
    fn stamps_are_deterministic_and_key_order_insensitive() {
        let a = compute_entry_hash(GENESIS_HASH, &payload()).unwrap();
        let mut shuffled = payload();
        let obj = shuffled.as_object_mut().unwrap();
        let mut items: Vec<_> = obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        items.sort_by(|a, b| b.0.cmp(&a.0)); // reverse order
        obj.clear();
        for (k, v) in items {
            obj.insert(k, v);
        }
        let b = compute_entry_hash(GENESIS_HASH, &shuffled).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn chain_of_three_verifies() {
        let p1 = payload();
        let h1 = compute_entry_hash(GENESIS_HASH, &p1).unwrap();
        let p2 = json!({"description": "Reversal of REF-1"});
        let h2 = compute_entry_hash(&h1, &p2).unwrap();
        let p3 = json!({"description": "Groceries"});
        let h3 = compute_entry_hash(&h2, &p3).unwrap();

        let digests = [
            EntryDigest {
                entry_id: "e1".into(),
                payload: p1.as_object().unwrap().clone(),
                prev_hash: GENESIS_HASH.into(),
                stored_hash: Some(h1.clone()),
            },
            EntryDigest {
                entry_id: "e2".into(),
                payload: p2.as_object().unwrap().clone(),
                prev_hash: h1.clone(),
                stored_hash: Some(h2.clone()),
            },
            EntryDigest {
                entry_id: "e3".into(),
                payload: p3.as_object().unwrap().clone(),
                prev_hash: h2.clone(),
                stored_hash: Some(h3.clone()),
            },
        ];
        let report = chain_state_from_digests(&digests);
        assert!(report.valid);
        assert_eq!(report.entries_checked, 3);
        assert_eq!(report.head_hash, h3);
    }

    #[test]
    fn tampered_payload_is_caught() {
        let p1 = json!({"description": "original"});
        let h1 = compute_entry_hash(GENESIS_HASH, &p1).unwrap();
        let tampered = json!({"description": "retro-edited"});
        let digests = [EntryDigest {
            entry_id: "e1".into(),
            payload: tampered.as_object().unwrap().clone(),
            prev_hash: GENESIS_HASH.into(),
            stored_hash: Some(h1),
        }];
        let report = chain_state_from_digests(&digests);
        assert!(!report.valid);
        assert!(matches!(report.errors[0], ChainError::HashMismatch { .. }));
    }

    #[test]
    fn broken_link_is_caught() {
        let p1 = json!({"description": "a"});
        let h1 = compute_entry_hash(GENESIS_HASH, &p1).unwrap();
        let p2 = json!({"description": "b"});
        let h2 = compute_entry_hash(&h1, &p2).unwrap();
        let wrong_prev = compute_entry_hash(GENESIS_HASH, &p2).unwrap();
        let digests = [
            EntryDigest {
                entry_id: "e1".into(),
                payload: p1.as_object().unwrap().clone(),
                prev_hash: GENESIS_HASH.into(),
                stored_hash: Some(h1),
            },
            EntryDigest {
                entry_id: "e2".into(),
                payload: p2.as_object().unwrap().clone(),
                prev_hash: wrong_prev.clone(),
                stored_hash: Some(h2),
            },
        ];
        let report = chain_state_from_digests(&digests);
        assert!(!report.valid);
        assert!(matches!(report.errors[0], ChainError::BrokenLink { .. }));
    }

    #[test]
    fn first_entry_must_chain_from_genesis() {
        let p1 = json!({"description": "first"});
        let fake_prev = compute_entry_hash("not-genesis", &p1).unwrap();
        let h1 = compute_entry_hash(&fake_prev, &p1).unwrap();
        let digests = [EntryDigest {
            entry_id: "e1".into(),
            payload: p1.as_object().unwrap().clone(),
            prev_hash: fake_prev,
            stored_hash: Some(h1),
        }];
        let report = chain_state_from_digests(&digests);
        assert!(!report.valid);
        assert!(matches!(report.errors[0], ChainError::BrokenLink { .. }));
    }
}
