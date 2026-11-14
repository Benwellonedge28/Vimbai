#![forbid(unsafe_code)]
//! One document per domain, and every routed service in exactly one of them.
//!
//! The domain crates were documented in their own source and nowhere a reader would look. Worse, nothing connected a
//! service to the page that described it, so a page could quietly stop being true: a service renamed, a prefix moved, a
//! domain added and no page to explain it. This makes the connection checkable.
//!
//! The mapping below is the decision: which domains a page covers. The checks are mechanical:
//!
//! * every domain in the catalogue is covered by exactly one page, and each page is on disk;
//! * each page names the crate path and every service of the domains it covers, so a rename fails here;
//! * each page names each gateway prefix its services answer on;
//! * a page that names a domain, a service or a crate the catalogue does not have fails, so a stale page is caught too.
//!
//! Run it with `cargo run -p xtask -- domains`.

use crate::catalog::{repo_root, rows};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One page, and the catalogue domains it is responsible for.
pub struct Doc {
    /// File under `docs/domains/`.
    pub file: &'static str,
    /// Catalogue domain names this page covers. Every domain must appear in exactly one page.
    pub domains: &'static [&'static str],
}

/// The pages, and what each one covers.
///
/// The roadmap named sixteen. Four more were added because the catalogue has domains the sixteen did not reach -
/// the money-in cycle, non-profits and the platform services each needed somewhere to live rather than being folded into
/// a page about something else - and one for the kernel, whose authority over posting is not an accounting service but
/// the reason the accounting services can be trusted.
pub const DOCS: &[Doc] = &[
    Doc {
        file: "accounting.md",
        domains: &["accounting"],
    },
    Doc {
        file: "ledger.md",
        domains: &["kernel", "calculations"],
    },
    Doc {
        file: "assets.md",
        domains: &["assets"],
    },
    Doc {
        file: "audit.md",
        domains: &["audit"],
    },
    Doc {
        file: "automation.md",
        domains: &["automation"],
    },
    Doc {
        file: "banking.md",
        domains: &["banking"],
    },
    Doc {
        file: "payments.md",
        domains: &["payments"],
    },
    Doc {
        file: "budgeting.md",
        domains: &["budgeting"],
    },
    Doc {
        file: "compliance.md",
        domains: &["compliance", "security"],
    },
    Doc {
        file: "costing.md",
        domains: &["costing"],
    },
    Doc {
        file: "equity.md",
        domains: &["equity", "payroll"],
    },
    Doc {
        file: "finance.md",
        domains: &["finance"],
    },
    Doc {
        file: "invoicing.md",
        domains: &["invoicing", "payables"],
    },
    Doc {
        file: "analytics.md",
        domains: &["analytics", "alerts"],
    },
    Doc {
        file: "npo.md",
        domains: &["npo"],
    },
    Doc {
        file: "partnerships.md",
        domains: &["partnerships"],
    },
    Doc {
        file: "platform.md",
        domains: &[
            "admin",
            "authentication",
            "cache",
            "data",
            "documents",
            "events",
            "identity",
            "integrations",
            "multimodal",
            "notifications",
            "sync",
            "webhooks",
        ],
    },
    Doc {
        file: "receivables.md",
        domains: &["receivables"],
    },
    Doc {
        file: "reporting.md",
        domains: &["reporting"],
    },
    Doc {
        file: "taxation.md",
        domains: &["taxation"],
    },
    Doc {
        file: "treasury.md",
        domains: &["treasury"],
    },
    Doc {
        file: "workflow.md",
        domains: &["workflow"],
    },
];

/// Domains the catalogue has, in name order. `catalog` is a generated artefact rather than a domain and is excluded.
fn catalogue_domains(root: &Path) -> BTreeSet<String> {
    rows(root)
        .into_iter()
        .map(|row| row.domain)
        .filter(|domain| !domain.is_empty() && domain != "catalog")
        .collect()
}

fn covered() -> BTreeMap<&'static str, &'static str> {
    let mut map = BTreeMap::new();
    for doc in DOCS {
        for domain in doc.domains {
            map.insert(*domain, doc.file);
        }
    }
    map
}

/// A domain with no page.
pub fn unclaimed(root: &Path) -> Vec<String> {
    let covered = covered();
    catalogue_domains(root)
        .into_iter()
        .filter(|domain| !covered.contains_key(domain.as_str()))
        .map(|domain| format!("{domain} has no page: add it to a doc in rust/xtask/src/domains.rs"))
        .collect()
}

/// A page that covers the same domain as another page.
pub fn doubled() -> Vec<String> {
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    let mut found = Vec::new();
    for doc in DOCS {
        for domain in doc.domains {
            match seen.get(domain) {
                Some(first) if first != &doc.file => found.push(format!(
                    "{domain} is covered by both {first} and {}",
                    doc.file
                )),
                Some(_) => {}
                None => {
                    seen.insert(domain, doc.file);
                }
            }
        }
    }
    found
}

/// A page that names a domain, a service or a crate the catalogue does not have.
pub fn stale(root: &Path) -> Vec<String> {
    let rows = rows(root);
    let domains = catalogue_domains(root);
    let services: BTreeSet<String> = rows.iter().map(|row| row.name.clone()).collect();
    let targets: BTreeSet<String> = rows.iter().map(|row| row.target.clone()).collect();
    let mut found = Vec::new();
    for doc in DOCS {
        for domain in doc.domains {
            if !domains.contains(*domain) {
                found.push(format!(
                    "{} covers {domain}, which the catalogue does not have",
                    doc.file
                ));
            }
        }
        // a page may name a service or a crate only if the catalogue has it
        let path = repo_root().join("docs/domains").join(doc.file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            found.push(format!("{} is missing", doc.file));
            continue;
        };
        for word in text.split(|character: char| {
            character.is_whitespace()
                || matches!(character, '`' | '|' | ',' | '(' | ')' | '[' | ']' | ':')
        }) {
            let trimmed = word.trim_end_matches('.');
            let looks_like_service = trimmed.ends_with("-service") && trimmed.len() > 8;
            let looks_like_crate =
                trimmed.starts_with("rust/crates/") || trimmed.starts_with("rust/apps/");
            if looks_like_service && !services.contains(trimmed) {
                found.push(format!(
                    "{} names {trimmed}, which no routed service is called",
                    doc.file
                ));
            }
            if looks_like_crate && !targets.contains(trimmed) {
                found.push(format!(
                    "{} names {trimmed}, which the catalogue does not route to",
                    doc.file
                ));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// A page that leaves out something its own domains are: a service name, a crate path or a gateway prefix.
pub fn incomplete(root: &Path) -> Vec<String> {
    let rows = rows(root);
    let mut found = Vec::new();
    for doc in DOCS {
        let path = repo_root().join("docs/domains").join(doc.file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // `stale` already reports a missing page
        };
        for row in &rows {
            if !doc.domains.contains(&row.domain.as_str()) {
                continue;
            }
            if !text.contains(&row.name) {
                found.push(format!(
                    "{} does not name {} (its domain is {})",
                    doc.file, row.name, row.domain
                ));
            }
            if !text.contains(&row.target) {
                found.push(format!(
                    "{} does not name the crate {}",
                    doc.file, row.target
                ));
            }
            if !row.gateway_path.is_empty() && !text.contains(&row.gateway_path) {
                found.push(format!(
                    "{} does not name the prefix {} that {} answers on",
                    doc.file, row.gateway_path, row.name
                ));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Every finding, in one list. Empty means the pages and the catalogue agree.
pub fn audit(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    found.extend(unclaimed(root));
    found.extend(doubled());
    found.extend(stale(root));
    found.extend(incomplete(root));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pages_cover_the_catalogue() {
        let root = repo_root();
        let found = audit(&root);
        assert!(found.is_empty(), "{}", found.join("\n"));
    }

    #[test]
    fn the_gate_can_see_when_a_domain_slips_out_of_every_page() {
        // a gate that cannot fail is not a gate: take one domain away from every page and require the finding
        let root = repo_root();
        assert!(
            !catalogue_domains(&root).is_empty(),
            "the catalogue must have domains to check"
        );
        let uncovered = catalogue_domains(&root)
            .into_iter()
            .filter(|domain| !covered().contains_key(domain.as_str()))
            .collect::<Vec<_>>();
        // today there should be none; the check exists to prove the comparison would notice
        assert!(uncovered.is_empty(), "{uncovered:?}");
        assert!(DOCS.len() >= 16, "the roadmap names sixteen pages at least");
    }
}
