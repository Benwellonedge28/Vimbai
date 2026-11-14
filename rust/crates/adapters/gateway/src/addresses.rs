//! The addresses clients call, and the addresses the engine answers.
//!
//! The gateway holds a routing table for one reason: some services publish under a prefix the engine does not use, so
//! `/book-sync/books` has to reach `/books`. A single machine still needs that much of a gateway, and it takes it from the
//! same manifest the cluster gateway reads rather than a second table copied out of it. Anything the manifest does not
//! say to strip is forwarded exactly as it arrived, which is the case for most services.

use std::cmp::Reverse;

use serde_json::Value;

/// The published prefix a service is reached on, and whether the engine wants that prefix taken off first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub prefix: String,
    pub strip: bool,
}

impl Mount {
    /// The address the engine should be asked for, given the address the client asked for.
    pub fn route(&self, path: &str) -> String {
        if !self.strip {
            return path.to_string();
        }
        match &path[self.prefix.len()..] {
            "" => "/".to_string(),
            rest if rest.starts_with('/') => rest.to_string(),
            rest => format!("/{rest}"),
        }
    }
}

/// Every service the manifest routes, longest prefix first so `/audit-sampling` is matched before `/audit`.
pub fn mounts(manifest: &str) -> Vec<Mount> {
    let Ok(parsed) = serde_json::from_str::<Value>(manifest) else {
        return Vec::new();
    };
    let Some(services) = parsed.get("services").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut mounts: Vec<Mount> = services
        .iter()
        .filter_map(|service| {
            Some(Mount {
                prefix: service.get("path")?.as_str()?.to_string(),
                strip: service
                    .get("strip_prefix")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    mounts.sort_by_key(|mount| Reverse(mount.prefix.len()));
    mounts
}

/// Which mount a path belongs to: the longest prefix that names it as a whole segment, so `/books` is not `/book-sync`
/// and `/audition` belongs to nobody.
pub fn mount_for<'a>(mounts: &'a [Mount], path: &str) -> Option<&'a Mount> {
    mounts.iter().find(|mount| {
        path == mount.prefix
            || path
                .strip_prefix(&mount.prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The address to ask the engine for, or the path unchanged when no service owns it.
pub fn engine_path(mounts: &[Mount], path: &str) -> String {
    match mount_for(mounts, path) {
        Some(mount) => mount.route(path),
        None => path.to_string(),
    }
}

/// The manifest the cluster gateway routes from, compiled in so an installed binary carries its own routing table and
/// cannot drift from the repository by being installed.
pub fn manifest() -> &'static str {
    include_str!("../../../../../gateway/go/config/services.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mounts_of(pairs: &[(&str, bool)]) -> Vec<Mount> {
        let services: Vec<Value> = pairs
            .iter()
            .map(|(path, strip)| serde_json::json!({ "path": path, "strip_prefix": strip }))
            .collect();
        let manifest = serde_json::to_string(&serde_json::json!({ "services": services })).unwrap();
        mounts(&manifest)
    }

    #[test]
    fn a_stripped_prefix_reaches_the_engines_own_address() {
        let mounts = mounts_of(&[("/book-sync", true)]);
        assert_eq!(engine_path(&mounts, "/book-sync/books"), "/books");
        assert_eq!(engine_path(&mounts, "/book-sync"), "/");
        assert_eq!(engine_path(&mounts, "/book-sync/"), "/");
    }

    #[test]
    fn an_unstripped_prefix_is_forwarded_untouched() {
        let mounts = mounts_of(&[("/departmental-accounting", false)]);
        assert_eq!(
            engine_path(&mounts, "/departmental-accounting/departments"),
            "/departmental-accounting/departments"
        );
    }

    #[test]
    fn a_prefix_matches_a_segment_not_a_word() {
        let mounts = mounts_of(&[("/audit", true), ("/audit-sampling", true)]);
        assert_eq!(engine_path(&mounts, "/audit-sampling/plans"), "/plans");
        assert_eq!(engine_path(&mounts, "/audit/controls"), "/controls");
        assert_eq!(engine_path(&mounts, "/audition"), "/audition");
    }

    #[test]
    fn the_manifest_the_binary_carries_routes_the_services_the_offline_profile_opens() {
        let mounts = mounts(manifest());
        assert!(
            mounts.len() > 100,
            "the manifest should describe every routed service, found {}",
            mounts.len()
        );
        // The core profile: four publish under their own prefix, and book-sync is the one the engine answers stripped.
        let expected = [
            ("/accounting", false),
            ("/departmental-accounting", false),
            ("/audit-compliance", true),
            ("/book-sync", true),
        ];
        for (prefix, strip) in expected {
            let mount = mounts
                .iter()
                .find(|mount| mount.prefix == prefix)
                .unwrap_or_else(|| panic!("{prefix} missing from the manifest"));
            assert_eq!(mount.strip, strip, "{prefix}");
        }
    }
}
