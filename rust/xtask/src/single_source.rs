#![forbid(unsafe_code)]
//! Rules that exist in one place, held to being in one place.
//!
//! Some facts are only true while they are written once. The ageing bands are one: what the business is owed and what it
//! owes are read by two files, and a boundary that moved in one of them would report the same debt at two different ages
//! on the same day, with nothing failing. Those boundaries now live in `accounting::ageing`, and this holds them there.

use std::path::{Path, PathBuf};

/// Where the ageing bands are declared, and the only file allowed to name their boundaries.
pub const AGEING: &str = "crates/domains/accounting/src/ageing.rs";

/// Every file outside the one place that still names an ageing band or draws its boundary.
///
/// A band is named by its key as the report prints it, and drawn by the day range that decides it, so both a second copy
/// of the names and a second copy of the boundaries are reported.
pub fn bands_declared_more_than_once(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for path in rust_files(&root.join("rust/crates/domains")) {
        let relative = path
            .strip_prefix(root.join("rust"))
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        if relative == AGEING {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let code = &text[..text.find("#[cfg(test)]").unwrap_or(text.len())];
        for (number, line) in code.lines().enumerate() {
            let comment = line.trim_start().starts_with("//");
            let names_a_band = ["\"over_90\"", "\"61_90\"", "\"31_60\""]
                .iter()
                .any(|band| line.contains(band));
            let draws_a_boundary = line.contains("61..=90") || line.contains("31..=60");
            if !comment && (names_a_band || draws_a_boundary) {
                found.push(format!("{relative}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    found
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            found.push(path);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sweep_reads_the_domains() {
        let root = crate::catalog::repo_root();
        let files = rust_files(&root.join("rust/crates/domains"));
        assert!(files.len() > 100, "the sweep found {} files", files.len());
        let declared = root.join("rust").join(AGEING);
        assert!(declared.exists(), "{} is gone", AGEING);
    }
}
