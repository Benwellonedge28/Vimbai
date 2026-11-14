#![forbid(unsafe_code)]
//! The worker process: tick, sleep, tick.
//!
//! All the interesting behaviour is in the library. This exists to be the thing a supervisor runs, and it deliberately
//! knows nothing about rules: it opens the same store the engine opens, asks the library for one tick, prints what
//! happened, and sleeps for an interval.
//!
//! Configuration, all optional except the store:
//!
//! - `VIMBAI_STORE_FILE` - the file-backed store the on-device runtime writes. Chosen before a database, so an offline
//!   machine cannot be talked into needing one.
//! - `VIMBAI_NEO4J_URL`, `VIMBAI_NEO4J_USER`, `VIMBAI_NEO4J_PASSWORD` - the database, when there is one.
//! - `VIMBAI_WORKER_INTERVAL_SECONDS` - how long to sleep between passes. Default 60.
//! - `VIMBAI_WORKER_ONCE` - `1` runs a single pass and exits, which is what a test or a cron job wants.

use std::sync::Arc;
use std::time::Duration;

/// The store, behind a handle so the same one serves every pass.
pub type GraphHandle = Arc<dyn store::Graph + Send + Sync>;

/// The store the worker and the engine both write, built the same way the engine builds it.
fn store_from_env() -> Option<GraphHandle> {
    if let Ok(path) = std::env::var("VIMBAI_STORE_FILE") {
        return match store::file::FileGraph::open(&path) {
            Ok(graph) => Some(Arc::new(graph) as GraphHandle),
            Err(error) => {
                eprintln!("cannot open the store at {path}: {error}");
                None
            }
        };
    }
    match store::Neo4jGraph::from_env() {
        Ok(graph) => Some(Arc::new(graph) as GraphHandle),
        Err(error) => {
            eprintln!("no store configured: {error}");
            None
        }
    }
}

fn interval() -> Duration {
    let seconds = std::env::var("VIMBAI_WORKER_INTERVAL_SECONDS")
        .ok()
        .and_then(|held| held.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(60);
    Duration::from_secs(seconds)
}

fn once() -> bool {
    matches!(
        std::env::var("VIMBAI_WORKER_ONCE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn pass(graph: &GraphHandle) {
    let now = common::clock::moment();
    match vimbai_worker::tick(graph.as_ref(), now) {
        Ok(answered) if answered.is_empty() => {}
        Ok(answered) => {
            for run in answered {
                println!("{run}");
            }
        }
        Err(problem) => eprintln!("a pass failed: {problem}"),
    }
}

fn main() {
    let Some(graph) = store_from_env() else {
        eprintln!("the worker has nowhere to work: no store is configured");
        std::process::exit(1);
    };

    if once() {
        pass(&graph);
        return;
    }

    let every = interval();
    println!("worker watching the store every {}s", every.as_secs());
    loop {
        pass(&graph);
        std::thread::sleep(every);
    }
}
