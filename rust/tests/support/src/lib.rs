//! A throwaway namespace in a real Postgres, for the tests that have to answer against a server.
//!
//! One test's leftovers must never decide another test's answer, so each test that needs a server asks for a namespace
//! of its own: it is created here, named after the process and the moment, and dropped when the guard goes out of scope.
//! The tests look for `VIMBAI_TEST_POSTGRES` and skip when it is absent, so a machine with no server still runs the
//! suite honestly instead of failing on something it was never given.
//!
//! Creating a namespace is the one step that is not private to a test. Every test binary in the workspace starts its own
//! at the same moment, so the server can answer "too many clients already" or "tuple concurrently updated" to a create
//! that would succeed a moment later. That is a fact about the server under load, not about the store, so the create is
//! retried a bounded number of times before it is allowed to fail a test.

#![forbid(unsafe_code)]

use std::time::Duration;

/// How many times a namespace create is tried before the server's answer is taken as final.
const ATTEMPTS: usize = 8;

/// The wait before the second attempt, doubled each time after that.
const FIRST_WAIT: Duration = Duration::from_millis(120);

/// A namespace of this test's own in a real Postgres, dropped when the guard goes.
pub struct TestSchema {
    /// Where the store connects: the caller's own settings with this namespace as the search path.
    dsn: String,
    schema: String,
}

impl TestSchema {
    /// Opens a namespace of its own, or answers `None` when no server was named for this run.
    ///
    /// `prefix` says which suite the namespace belongs to, so a namespace left behind by a killed run names its owner.
    pub fn open(prefix: &str) -> Option<Self> {
        let base = std::env::var("VIMBAI_TEST_POSTGRES").ok()?;
        let schema = format!("{prefix}_{}_{}", std::process::id(), unique());
        create_schema(&base, &schema);
        // search_path makes the store's own statements land in this namespace without the store having to know about it
        let dsn = format!("{base} options='-c search_path={schema}'");
        Some(Self { dsn, schema })
    }

    /// Where a store should connect to find itself inside this namespace.
    pub fn dsn(&self) -> &str {
        &self.dsn
    }

    /// The namespace's own name, for a test that wants to look at it directly.
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// The caller's settings with the namespace left out, for the statements that run outside it.
    fn base(&self) -> &str {
        self.dsn.split(" options=").next().unwrap_or(&self.dsn)
    }
}

impl Drop for TestSchema {
    fn drop(&mut self) {
        if let Ok(mut admin) = postgres::Client::connect(self.base(), postgres::NoTls) {
            let _ = admin.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE;", self.schema));
        }
    }
}

/// Creates the namespace, retrying the answers that mean "the server is busy", never the ones that mean "this is wrong".
fn create_schema(base: &str, schema: &str) {
    let mut wait = FIRST_WAIT;
    let mut last = String::new();
    for attempt in 1..=ATTEMPTS {
        match postgres::Client::connect(base, postgres::NoTls) {
            Ok(mut admin) => match admin.batch_execute(&format!("CREATE SCHEMA {schema};")) {
                Ok(()) => return,
                // a namespace of this name already exists: the name is the process and the moment, so this is not a
                // collision to retry past but something to see
                Err(error) if attempt == ATTEMPTS => {
                    panic!("cannot create {schema} after {ATTEMPTS} attempts: {error}")
                }
                Err(error) => last = error.to_string(),
            },
            Err(error) if attempt == ATTEMPTS => {
                panic!("cannot reach {base} after {ATTEMPTS} attempts: {error}")
            }
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(wait);
        wait *= 2;
    }
    panic!("cannot create {schema}: {last}");
}

/// A number no other test in this run will pick, even one starting in the same nanosecond: the counter keeps two calls in
/// one process apart, and the clock keeps two processes apart.
fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    // strictly increasing, so a name is never reused inside one process even if the clock stands still
    let mut previous = LAST.load(Ordering::Relaxed);
    loop {
        let next = if now > previous { now } else { previous + 1 };
        match LAST.compare_exchange_weak(previous, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(seen) => previous = seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_names_asked_for_at_the_same_moment_are_different() {
        let names: Vec<u64> = (0..64).map(|_| unique()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "a name was handed out twice");
    }

    #[test]
    fn names_only_ever_grow_so_a_later_test_cannot_land_on_an_earlier_one() {
        let mut previous = unique();
        for _ in 0..32 {
            let next = unique();
            assert!(next > previous, "{next} did not come after {previous}");
            previous = next;
        }
    }

    #[test]
    fn without_a_server_the_helper_says_so_instead_of_panicking() {
        // the suite runs with and without a server; this asserts the answer is an honest absence
        if std::env::var("VIMBAI_TEST_POSTGRES").is_ok() {
            return;
        }
        assert!(TestSchema::open("vimbai_absent").is_none());
    }

    #[test]
    fn eight_namespaces_asked_for_at_once_are_eight_namespaces() {
        // the shape of a workspace test run: many binaries opening a namespace in the same moment, which is what the
        // server used to refuse
        if std::env::var("VIMBAI_TEST_POSTGRES").is_err() {
            return;
        }
        let opened: Vec<TestSchema> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| TestSchema::open("vimbai_apart").expect("a server")))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a namespace"))
                .collect()
        });
        let mut names: Vec<String> = opened.iter().map(|one| one.schema().to_owned()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 8, "two tests were handed the same namespace");
        for one in &opened {
            // each namespace is real: the server answers for it
            let mut client = postgres::Client::connect(one.dsn(), postgres::NoTls)
                .expect("a client inside the namespace");
            let row = client
                .query_one("SELECT current_schema();", &[])
                .expect("the namespace answers");
            let found: String = row.get(0);
            assert_eq!(&found, one.schema());
        }
    }
}
