#![forbid(unsafe_code)]
//! The on-device runtime: the whole book behind one process, on a machine with no server.
//!
//! It is the engine with a data directory. Everything it answers - accounts, journals, departments, books, sign-in -
//! comes from the same handlers the cluster serves, so an offline machine cannot disagree with a connected one about
//! what a sale did to the ledger. What this binary adds is only what a single machine needs: a place to keep the store,
//! a default port, and a health door that says which profile is running.
//!
//!     vimbai-local                        # the core bookkeeping profile
//!     vimbai-local --profile full         # every address the gateway routes
//!     vimbai-local --port 9000 --data-dir ~/vimbai-data

use std::net::SocketAddr;
use std::path::PathBuf;

use vimbai_gateway::addresses;
use vimbai_gateway::gateway;
use vimbai_gateway::Gateway;

/// The bookkeeping loop an offline machine actually needs: the chart of accounts and the double-entry journal, the
/// departmental books, the audit trail, the door you sign in through, and the Book itself. Every one of them is served
/// by the engine, so this is a list of what is exercised rather than a list of what is loaded.
const CORE_MEMBERS: [&str; 5] = [
    "accounting",
    "departmental-accounting",
    "audit-compliance",
    "identity",
    "book-sync",
];

fn main() {
    let options = Options::from_args(std::env::args().skip(1));
    // The store has to be chosen before the graph is built, and the graph is built from the environment, so the
    // runtime hands the engine its answers rather than reaching into it.
    let data_dir = options.store_file();
    // Signing somebody in has to work on a machine that was told nothing, and it has to keep working tomorrow, so the
    // secret lives beside the store rather than in the environment. Created once, readable only by its owner.
    std::env::set_var("VIMBAI_JWT_SECRET", secret_in(&data_dir));
    std::env::set_var("VIMBAI_STORE_FILE", &data_dir);
    std::env::set_var("VIMBAI_LOCAL_PROFILE", &options.profile);
    std::env::set_var("VIMBAI_LOCAL_MEMBERS", CORE_MEMBERS.join(","));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("cannot start the runtime");
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
    // The port it actually landed on, not the one it was asked for: a caller may say port 0 and let the operating system
    // choose, which is how a test avoids racing another process for a number between finding it free and binding it.
    let address = listener.local_addr().unwrap_or(address);

    println!(
        "vimbai-local listening on {address} (profile {}, data in {})",
        options.profile,
        options.data_dir.display()
    );
    let engine = api::router();
    let gateway = Gateway {
        secret: std::env::var("VIMBAI_JWT_SECRET").unwrap_or_default(),
        mounts: addresses::mounts(addresses::manifest()),
        engine,
    };
    // the door goes outside the gateway: the bound is on work the machine is doing, and the gateway's own checks are work
    let served = api::governed(engine_with_gateway(gateway));
    axum::serve(listener, served)
        .await
        .unwrap_or_else(|err| panic!("the runtime stopped: {err}"));
}

struct Options {
    profile: String,
    bind: String,
    data_dir: PathBuf,
}

impl Options {
    fn from_args(args: impl Iterator<Item = String>) -> Self {
        let mut options = Options {
            profile: "core".to_string(),
            bind: "127.0.0.1:8000".to_string(),
            data_dir: default_data_dir(),
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let mut value = || {
                args.next()
                    .unwrap_or_else(|| panic!("{arg} needs a value after it"))
            };
            match arg.as_str() {
                "--profile" => options.profile = value(),
                "--port" => options.bind = format!("127.0.0.1:{}", value()),
                "--bind" => options.bind = value(),
                "--data-dir" => options.data_dir = PathBuf::from(value()),
                "--help" | "-h" => {
                    println!(
                        "vimbai-local [--profile core|full] [--port 8000] [--bind host:port] [--data-dir DIR]"
                    );
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

    /// One file, next to nothing else, so a machine can be backed up by copying a directory.
    ///
    /// SQLite rather than the JSON file this used to be: the device writes while somebody is using it, and a whole-file
    /// rewrite per change is the thing that eventually loses a Book to a crash.
    fn store_file(&self) -> PathBuf {
        std::fs::create_dir_all(&self.data_dir)
            .unwrap_or_else(|err| panic!("cannot create {}: {err}", self.data_dir.display()));
        self.data_dir.join("store.db")
    }
}

/// The engine, behind the gateway rules the cluster's gateway would otherwise apply.
fn engine_with_gateway(gateway: Gateway) -> axum::Router {
    axum::Router::new()
        .fallback_service(gateway.engine.clone())
        .layer(axum::middleware::from_fn_with_state(
            gateway.clone(),
            gateway::check,
        ))
}

/// A stable signing secret for this machine, created on first boot and kept with the data it signs for.
fn secret_in(data_dir: &std::path::Path) -> String {
    let path = data_dir.with_file_name("jwt_secret");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let secret = random_secret();
    std::fs::write(&path, &secret)
        .unwrap_or_else(|err| panic!("cannot write {}: {err}", path.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    secret
}

/// 48 bytes from the operating system's randomness, base64url without padding - the same shape as a token secret a
/// deployment would set, so nothing downstream has to know this one came from a file.
fn random_secret() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 48];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .expect("the operating system has no randomness to draw from");
    bytes.iter().fold(String::new(), |mut out, byte| {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        out.push(ALPHABET[(byte & 0x3f) as usize] as char);
        out
    })
}

/// `VIMBAI_DATA_DIR` when a machine wants to say where; otherwise the usual place for this platform's data, so that
/// `vimbai-local` twice in a row finds yesterday's book without being told anything.
fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("VIMBAI_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".vimbai");
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile).join(".vimbai");
    }
    PathBuf::from(".vimbai")
}
