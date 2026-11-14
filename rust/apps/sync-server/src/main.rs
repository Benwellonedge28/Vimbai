#![forbid(unsafe_code)]
//! The sync server: the Book surface a device reaches, and nothing else.
//!
//! The app is offline-first. A device that has been away has a queue of encrypted entries to push and a sequence to pull,
//! and it wants to do that against something small, quick and always there - not against the process that also posts
//! journals. So this binary serves that surface from the same handlers the engine uses: no second implementation of a
//! push, no second idea of who is a member of a Book, just a narrower door.
//!
//!     vimbai-sync --port 8100 --data-dir /var/lib/vimbai
//!
//! Behind no gateway at all, it applies the gateway's own rules (see `vimbai_gateway`), because a device presents a token
//! and the engine expects `X-User-Id` to have been put there by somebody who verified it.

mod surface;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use vimbai_gateway::addresses;
use vimbai_gateway::gateway;
use vimbai_gateway::Gateway;

fn main() {
    let options = Options::from_args(std::env::args().skip(1));
    let store = options.store_file();
    std::env::set_var("VIMBAI_JWT_SECRET", options.secret());
    std::env::set_var("VIMBAI_STORE_FILE", &store);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("cannot start the sync server");
    runtime.block_on(serve(options));
}

async fn serve(options: Options) {
    let address: SocketAddr = options
        .bind
        .parse()
        .unwrap_or_else(|_| panic!("cannot read the address {} as host:port", options.bind));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap_or_else(|err| panic!("cannot bind {address}: {err}"));

    println!(
        "vimbai-sync listening on {address} (data in {})",
        options.data_dir.display()
    );
    let engine = api::router();
    let gateway = Gateway {
        secret: std::env::var("VIMBAI_JWT_SECRET").unwrap_or_default(),
        mounts: addresses::mounts(addresses::manifest()),
        engine: engine.clone(),
    };
    let served = engine.layer(axum::middleware::from_fn(surface_only)).layer(
        axum::middleware::from_fn_with_state(gateway, gateway::check),
    );
    axum::serve(listener, served)
        .await
        .unwrap_or_else(|err| panic!("the sync server stopped: {err}"));
}

/// Refuse anything outside the sync surface, before the gateway rules run so a client learns why rather than getting a
/// 401 about a door that was never going to open.
async fn surface_only(request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if surface::serves(&path) {
        return next.run(request).await;
    }
    (
        axum::http::StatusCode::NOT_FOUND,
        axum::Json(json!({
            "detail": surface::outside(&path),
            "error": surface::outside(&path),
        })),
    )
        .into_response()
}

struct Options {
    bind: String,
    data_dir: PathBuf,
}

impl Options {
    fn from_args(args: impl Iterator<Item = String>) -> Self {
        let mut options = Options {
            bind: "127.0.0.1:8100".to_string(),
            data_dir: std::env::temp_dir().join("vimbai-sync"),
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let mut value = || {
                args.next()
                    .unwrap_or_else(|| panic!("{arg} needs a value after it"))
            };
            match arg.as_str() {
                "--port" => options.bind = format!("127.0.0.1:{}", value()),
                "--bind" => options.bind = value(),
                "--data-dir" => options.data_dir = PathBuf::from(value()),
                "--help" | "-h" => {
                    println!("vimbai-sync [--port 8100] [--bind host:port] [--data-dir DIR]");
                    std::process::exit(0);
                }
                other => {
                    eprintln!("unknown argument: {other}");
                    std::process::exit(2);
                }
            }
        }
        options
    }

    /// One file, so a sync server can be backed up by copying a directory - the same shape the on-device runtime keeps.
    fn store_file(&self) -> PathBuf {
        std::fs::create_dir_all(&self.data_dir)
            .unwrap_or_else(|err| panic!("cannot create {}: {err}", self.data_dir.display()));
        self.data_dir.join("store.json")
    }

    /// The secret that signs the tokens this server issues, kept beside the data it signs for and created once.
    fn secret(&self) -> String {
        let path = self.data_dir.with_file_name("jwt_secret");
        if let Ok(existing) = std::fs::read_to_string(&path) {
            let trimmed = existing.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        let secret = format!(
            "{:032x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default()
        );
        let _ = std::fs::write(&path, &secret);
        secret
    }
}
