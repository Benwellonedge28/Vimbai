#![forbid(unsafe_code)]
//! Serves the calculation library over HTTP.
//!
//! VIMBAI_API_ADDR chooses the bind address; the default matches the port the
//! Python services sit behind in development.

#[tokio::main]
async fn main() {
    let address = std::env::var("VIMBAI_API_ADDR").unwrap_or_else(|_| "0.0.0.0:8099".to_string());
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .unwrap_or_else(|err| panic!("cannot bind {address}: {err}"));
    println!("vimbai-api listening on {address}");
    axum::serve(listener, vimbai_api::router())
        .await
        .unwrap_or_else(|err| panic!("server stopped: {err}"));
}
