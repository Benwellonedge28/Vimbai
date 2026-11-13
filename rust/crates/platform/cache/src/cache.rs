#![forbid(unsafe_code)]
//! cache-service: keyed entries with a life, a size limit, and the counts a cache keeps of itself.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an entry is held with the moment it runs out. Reading it before then answers the value and how long is left;
//!   reading it after drops it and answers as a miss, so a stale value is never handed back;
//! * a hit marks the entry as the most recently used, and when the cache is full it is the least recently used entry
//!   that goes - not the oldest written;
//! * writing an entry answers the value straight back with the life it was given, and an entry written again under the
//!   same key replaces the one that was there;
//! * removing one entry says whether there was one to remove, and clearing says how many went;
//! * the counts are the cache's own: hits, misses, evictions and writes, with the hit rate as a percentage of reads.
//!
//! The clock is taken from the request rather than read here, so a cache can be reasoned about and tested exactly.
//! Entries live on the store rather than in one process's memory, so every replica sees the same cache.

use serde_json::{json, Value};

use vimbai_store::read::{field, refused, text, whole, whole_signed};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// One held value.
pub const ENTRY: RecordKind = RecordKind::new("CacheEntry", "OWNS_CACHE_ENTRY");

/// The cache's own counts.
pub const COUNTS: RecordKind = RecordKind::new("CacheCounter", "OWNS_CACHE_COUNTER");

/// How many entries are held before the least recently used one is dropped.
pub const MAX_SIZE: u32 = 10_000;

/// How long an entry lives when it is given no life of its own.
pub const DEFAULT_TTL: i64 = 3_600;

fn counts(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let held = Records::new(graph, COUNTS).list(scope, 10)?;
    Ok(held
        .into_iter()
        .next()
        .unwrap_or_else(|| json!({"id": "", "hits": 0, "misses": 0, "evictions": 0, "sets": 0})))
}

fn count_up(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    held: &Value,
    change: Value,
) -> Result<(), StoreError> {
    let counts = Records::new(graph, COUNTS);
    let mut fields = json!({
        "hits": whole_signed(held, "hits").unwrap_or(0),
        "misses": whole_signed(held, "misses").unwrap_or(0),
        "evictions": whole_signed(held, "evictions").unwrap_or(0),
        "sets": whole_signed(held, "sets").unwrap_or(0),
    });
    for (name, by) in change.as_object().cloned().unwrap_or_default() {
        let before = fields[&name].as_i64().unwrap_or(0);
        fields[&name] = json!(before + by.as_i64().unwrap_or(0));
    }
    match field(held, "id") {
        "" => {
            fields["id"] = json!(vimbai_common::ids::service_id());
            counts.create(scope, &fields)?;
        }
        existing => {
            counts.update(scope, existing, &fields)?;
        }
    }
    Ok(())
}

fn entries(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, ENTRY).list(scope, 100_000)
}

fn answer(key: &str, value: Option<Value>, found: bool, ttl_remaining: i64) -> Value {
    json!({
        "key": key,
        "value": value,
        "found": found,
        "ttl_remaining": ttl_remaining,
    })
}

/// How many seconds are left on an entry at `now`, zero once it has run out.
fn remaining(entry: &Value, now: i64) -> i64 {
    let runs_out = whole_signed(entry, "expires_at").unwrap_or(0);
    (runs_out - now).max(0)
}

/// `GET /cache/{key}`: a value and how long is left on it, or a miss.
pub fn read(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    key: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let now = whole_signed(payload, "now").map_err(|_| refused("now is required"))?;
    let held = entries(graph, scope)?;
    let counts = counts(graph, scope)?;
    match held.iter().find(|entry| field(entry, "key") == key) {
        Some(entry) if remaining(entry, now) > 0 => {
            // a hit marks the entry as the most recently used
            Records::new(graph, ENTRY).update(
                scope,
                field(entry, "id"),
                &json!({"touched_at": now}),
            )?;
            count_up(graph, scope, &counts, json!({"hits": 1}))?;
            Ok(answer(
                key,
                entry.get("value").cloned(),
                true,
                remaining(entry, now),
            ))
        }
        Some(entry) => {
            // it ran out: it goes, and the read answers as a miss rather than a stale value
            Records::new(graph, ENTRY).delete(scope, field(entry, "id"))?;
            count_up(graph, scope, &counts, json!({"misses": 1}))?;
            Ok(answer(key, None, false, 0))
        }
        None => {
            count_up(graph, scope, &counts, json!({"misses": 1}))?;
            Ok(answer(key, None, false, 0))
        }
    }
}

/// `POST /cache`: hold a value under a key.
pub fn write(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let key = text(payload, "key");
    if key.is_empty() {
        return Err(refused("key is required"));
    }
    let now = whole_signed(payload, "now").map_err(|_| refused("now is required"))?;
    let ttl = match whole_signed(payload, "ttl") {
        Ok(given) if given > 0 => given,
        _ => DEFAULT_TTL,
    };
    let max_size = match whole(payload, "max_size") {
        0 => MAX_SIZE,
        given => given,
    };
    let records = Records::new(graph, ENTRY);
    let held = entries(graph, scope)?;
    let counts = counts(graph, scope)?;

    // writing under a key that is already held replaces it rather than holding two
    let existing = held.iter().find(|entry| field(entry, "key") == key);
    let mut evicted = 0_i64;
    if existing.is_none() {
        // the cache makes room first, and it is the least recently used entry that goes
        let mut by_touch: Vec<&Value> = held.iter().collect();
        by_touch.sort_by_key(|entry| whole_signed(entry, "touched_at").unwrap_or(0));
        let mut over = held.len() as i64 - (max_size as i64 - 1);
        for entry in by_touch {
            if over <= 0 {
                break;
            }
            records.delete(scope, field(entry, "id"))?;
            evicted += 1;
            over -= 1;
        }
    }
    let fields = json!({
        "key": key,
        "value": payload.get("value").cloned().unwrap_or(Value::Null),
        "expires_at": now + ttl,
        "created_at": now,
        "touched_at": now,
    });
    match existing {
        Some(entry) => {
            records.update(scope, field(entry, "id"), &fields)?;
        }
        None => {
            let mut fields = fields;
            fields["id"] = json!(vimbai_common::ids::service_id());
            records.create(scope, &fields)?;
        }
    }
    let mut change = json!({"sets": 1});
    if evicted > 0 {
        change["evictions"] = json!(evicted);
    }
    count_up(graph, scope, &counts, change)?;
    Ok(answer(&key, payload.get("value").cloned(), true, ttl))
}

/// `DELETE /cache/{key}`: drop one entry, saying whether there was one.
pub fn remove(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    key: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, ENTRY);
    match entries(graph, scope)?
        .iter()
        .find(|entry| field(entry, "key") == key)
    {
        Some(entry) => {
            records.delete(scope, field(entry, "id"))?;
            Ok(json!({"key": key, "deleted": true}))
        }
        None => Ok(json!({"key": key, "deleted": false})),
    }
}

/// `DELETE /cache`: empty the cache, saying how many went.
pub fn clear(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let records = Records::new(graph, ENTRY);
    let held = entries(graph, scope)?;
    let mut cleared = 0_i64;
    for entry in &held {
        records.delete(scope, field(entry, "id"))?;
        cleared += 1;
    }
    Ok(json!({"cleared": cleared}))
}

/// `GET /stats`: how the cache is doing, and how much of it is in use.
pub fn cache_totals(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let now = whole_signed(payload, "now").unwrap_or(0);
    let held = entries(graph, scope)?;
    let counts = counts(graph, scope)?;
    // an entry that has run out is not in the cache any more, so it is not counted as held
    let live = held
        .iter()
        .filter(|entry| remaining(entry, now) > 0)
        .count();
    let hits = whole(&counts, "hits") as f64;
    let misses = whole(&counts, "misses") as f64;
    let reads = hits + misses;
    let hit_rate = if reads > 0.0 {
        (hits / reads * 100.0 * 100.0).round() / 100.0
    } else {
        0.0
    };
    Ok(json!({
        "cache_size": live,
        "max_size": match whole(payload, "max_size") { 0 => MAX_SIZE, given => given },
        "stats": {
            "hits": whole(&counts, "hits"),
            "misses": whole(&counts, "misses"),
            "evictions": whole(&counts, "evictions"),
            "sets": whole(&counts, "sets"),
        },
        "hit_rate_pct": hit_rate,
        "oldest_touched": oldest_touched(&held, now),
    }))
}

/// The longest an entry has gone untouched, when anything is held.
fn oldest_touched(held: &[Value], now: i64) -> Option<i64> {
    held.iter()
        .filter(|entry| remaining(entry, now) > 0)
        .map(|entry| whole_signed(entry, "touched_at").unwrap_or(0))
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn mine() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn put(graph: &MemoryGraph, key: &str, value: Value, now: i64, ttl: i64) {
        write(
            graph,
            &mine(),
            &json!({"key": key, "value": value, "now": now, "ttl": ttl}),
        )
        .expect("a write");
    }

    #[test]
    fn a_value_is_held_for_its_life_and_then_it_is_gone() {
        let graph = MemoryGraph::new();
        put(&graph, "rates", json!({"usd": 1.25}), 1000, 60);

        let hit = read(&graph, &mine(), "rates", &json!({"now": 1030})).expect("a read");
        assert_eq!(hit["found"], json!(true));
        assert_eq!(hit["value"], json!({"usd": 1.25}));
        // half its life is left
        assert_eq!(hit["ttl_remaining"], json!(30));

        // once it has run out the read is a miss, and the value is not handed back stale
        let stale = read(&graph, &mine(), "rates", &json!({"now": 1061})).expect("a read");
        assert_eq!(stale["found"], json!(false));
        assert_eq!(stale["value"], Value::Null);
        // and it was dropped rather than left to be read again
        let again = read(&graph, &mine(), "rates", &json!({"now": 1061})).expect("a read");
        assert_eq!(again["found"], json!(false));
        let totals = cache_totals(&graph, &mine(), &json!({"now": 1061})).expect("a count");
        assert_eq!(totals["stats"]["hits"], json!(1));
        assert_eq!(totals["stats"]["misses"], json!(2));
        assert_eq!(totals["cache_size"], json!(0));
    }

    #[test]
    fn a_read_marks_an_entry_as_the_most_recently_used() {
        let graph = MemoryGraph::new();
        put(&graph, "old", json!(1), 1000, 600);
        put(&graph, "new", json!(2), 1010, 600);
        // the older entry is read, so it is no longer the one to drop
        read(&graph, &mine(), "old", &json!({"now": 1020})).expect("a read");

        // writing a third with room for two makes room by dropping the least recently used
        write(
            &graph,
            &mine(),
            &json!({"key": "fresh", "value": 3, "now": 1030, "ttl": 600, "max_size": 2}),
        )
        .expect("a write");

        let held: Vec<String> = entries(&graph, &mine())
            .expect("entries")
            .iter()
            .map(|entry| field(entry, "key").to_string())
            .collect();
        assert!(held.contains(&"old".to_string()), "the read entry stayed");
        assert!(held.contains(&"fresh".to_string()));
        assert!(
            !held.contains(&"new".to_string()),
            "the least recently used went"
        );
        let totals = cache_totals(&graph, &mine(), &json!({"now": 1030})).expect("a count");
        assert_eq!(totals["stats"]["evictions"], json!(1));
        assert_eq!(totals["cache_size"], json!(2));
    }

    #[test]
    fn writing_the_same_key_again_replaces_what_was_there() {
        let graph = MemoryGraph::new();
        put(&graph, "rates", json!("first"), 1000, 600);
        put(&graph, "rates", json!("second"), 1010, 600);
        assert_eq!(entries(&graph, &mine()).expect("entries").len(), 1);
        let held = read(&graph, &mine(), "rates", &json!({"now": 1020})).expect("a read");
        assert_eq!(held["value"], json!("second"));
        // a write answers the value straight back with the life it was given
        let written = write(
            &graph,
            &mine(),
            &json!({"key": "tally", "value": 7, "now": 1020}),
        )
        .expect("a write");
        assert_eq!(written["found"], json!(true));
        assert_eq!(written["ttl_remaining"], json!(DEFAULT_TTL));
    }

    #[test]
    fn removing_and_clearing_say_what_went() {
        let graph = MemoryGraph::new();
        put(&graph, "one", json!(1), 1000, 600);
        put(&graph, "two", json!(2), 1000, 600);
        let gone = remove(&graph, &mine(), "one").expect("a removal");
        assert_eq!(gone["deleted"], json!(true));
        // removing what is not held says so rather than pretending
        let missing = remove(&graph, &mine(), "one").expect("a removal");
        assert_eq!(missing["deleted"], json!(false));
        let cleared = clear(&graph, &mine()).expect("a clear");
        assert_eq!(cleared["cleared"], json!(1));
        // and a write with no key is refused
        assert!(write(&graph, &mine(), &json!({"value": 1, "now": 1000})).is_err());
    }

    #[test]
    fn the_counts_are_the_caches_own() {
        let graph = MemoryGraph::new();
        put(&graph, "rates", json!(1), 1000, 600);
        read(&graph, &mine(), "rates", &json!({"now": 1010})).expect("a read");
        read(&graph, &mine(), "rates", &json!({"now": 1020})).expect("a read");
        read(&graph, &mine(), "absent", &json!({"now": 1020})).expect("a read");
        let totals = cache_totals(&graph, &mine(), &json!({"now": 1020})).expect("a count");
        assert_eq!(totals["stats"]["sets"], json!(1));
        assert_eq!(totals["stats"]["hits"], json!(2));
        assert_eq!(totals["stats"]["misses"], json!(1));
        // two hits of three reads
        assert_eq!(totals["hit_rate_pct"], json!(66.67));
        assert_eq!(totals["cache_size"], json!(1));

        // a cache nobody has used has no hit rate rather than a made-up one
        let empty = MemoryGraph::new();
        let none = cache_totals(&empty, &mine(), &json!({"now": 0})).expect("a count");
        assert_eq!(none["hit_rate_pct"], json!(0.0));
    }
}
