#![forbid(unsafe_code)]
//! Serves the calculation library over HTTP.
//!
//! VIMBAI_API_ADDR chooses the bind address; the default matches the port the
//! Python services sit behind in development.

/// How many requests the engine works on at once.
///
/// `#[tokio::main]` would pick this for us, but a deployment needs to say so itself: a node with a small store pool wants
/// fewer workers than it has cores, and a node fronting many Books wants more. One worker per core is the default because
/// it is right more often than any other guess.
fn workers() -> usize {
    std::env::var("VIMBAI_API_WORKERS")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .filter(|count| *count > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(4)
        })
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers())
        .enable_all()
        .build()
        .unwrap_or_else(|err| panic!("cannot start the engine's runtime: {err}"));
    runtime.block_on(serve());
    // Dropping the runtime here rather than letting `main` end is deliberate: everything the engine holds - the store, its
    // pool, the broker - is let go in one known order while there is still a process to complain if something goes wrong.
    // The deployment store closes its connections on a thread of its own, so a shutdown inside the runtime cannot drive a
    // synchronous client's destructor from the wrong place.
    drop(runtime);
}

async fn serve() {
    let address = std::env::var("VIMBAI_API_ADDR").unwrap_or_else(|_| "0.0.0.0:8099".to_string());
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .unwrap_or_else(|err| panic!("cannot bind {address}: {err}"));
    let address = listener.local_addr().unwrap_or_else(|_| {
        address
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:0".parse().expect("an address"))
    });
    let limits = vimbai_api::limits::Limits::from_env();
    println!(
        "vimbai-api listening on {address} (workers {}, at most {} requests at once, {}ms deadline)",
        workers(),
        limits.in_flight,
        limits.timeout.as_millis()
    );
    axum::serve(listener, vimbai_api::governed_router())
        .await
        .unwrap_or_else(|err| panic!("server stopped: {err}"));
}
