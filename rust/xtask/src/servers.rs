#![forbid(unsafe_code)]

//! Every process that serves HTTP, and whether it serves behind the door.
//!
//! `limits::govern` is a layer a router has to be wrapped in. Nothing in the type system says a server did it, so a new
//! binary can ship with an unbounded door and no one notices until a burst of traffic does. This is the check that says it
//! out loud: a `main.rs` that calls `axum::serve` must hand it a router the door wrapped.

use std::path::Path;

/// Every process under `rust/apps` that serves HTTP, with its source read as text.
pub fn servers(root: &Path) -> Vec<(String, String)> {
    let apps = root.join("rust/apps");
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&apps) else {
        return found;
    };
    for entry in entries.filter_map(Result::ok) {
        let main = entry.path().join("src/main.rs");
        let Ok(text) = std::fs::read_to_string(&main) else {
            continue;
        };
        if !text.contains("axum::serve") {
            continue;
        }
        found.push((
            main.strip_prefix(&apps)
                .unwrap_or(&main)
                .to_string_lossy()
                .into_owned(),
            text,
        ));
    }
    found.sort();
    found
}

/// Servers that serve HTTP without handing their router to the door.
pub fn servers_missing_the_door(root: &Path) -> Vec<String> {
    servers(root)
        .into_iter()
        // `governed_router()` and `governed(..)` are the same door; a server naming either has walked through it
        .filter(|(_, text)| !text.contains("governed"))
        .map(|(name, _)| name)
        .collect()
}
