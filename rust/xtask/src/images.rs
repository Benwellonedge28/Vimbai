#![forbid(unsafe_code)]

//! The images the cluster pulls, against the images the repository publishes.
//!
//! `docker-publish.yml` looked for a Dockerfile inside a changed top-level directory - the shape the Python microservices had -
//! and none exists here, so every matrix entry skipped itself and the job was green while publishing nothing at all. Meanwhile
//! four Deployments asked ghcr.io for images nobody had pushed, and a cluster would have answered ImagePullBackOff for all four.
//! Neither half said anything, because nothing compared them.
//!
//! Two directions, both cheap: an image a manifest pulls has to be published by a workflow whose Dockerfile exists, and an image a
//! workflow publishes has to be pulled by something. The third-party images are recognised by their registry rather than listed,
//! so a new `postgres:16` needs no edit here.

use std::collections::BTreeSet;
use std::path::Path;

/// The registry path this repository publishes under. An image outside it is somebody else's to build.
const OWN_REGISTRY: &str = "ghcr.io/benwellonedge28/vimbai/";

/// What is wrong: an image pulled that nothing publishes, or published that nothing pulls.
pub fn unpublished_images(root: &Path) -> Vec<String> {
    let published = published(root);
    let pulled = pulled(root);
    if pulled.is_empty() {
        return vec!["no manifest pulls an image, so the scan is not reading them".to_string()];
    }
    if published.is_empty() {
        return vec!["no workflow publishes an image, so the scan is not reading them".to_string()];
    }
    let mut problems = Vec::new();
    for dockerfile in published_dockerfiles(root) {
        if !root.join(&dockerfile).exists() {
            problems.push(format!(
                "a workflow builds from {dockerfile}, which is not there, so the build fails at the far end of a queue"
            ));
        }
    }
    for (image, where_from) in &pulled {
        if !published.contains(image) {
            problems.push(format!(
                "{where_from} pulls {image}, which no workflow publishes"
            ));
        }
    }
    for image in &published {
        if !pulled.iter().any(|(name, _)| name == image) {
            problems.push(format!(
                "a workflow publishes {image} and nothing pulls it, so the build is spent on an image nobody runs"
            ));
        }
    }
    problems
}

/// The repository's images a manifest asks for, with the file that asks.
pub fn pulled(root: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for file in manifests(root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            // a container entry is a list item, so the key arrives behind `- `
            let trimmed = line.trim_start();
            let Some(rest) = trimmed
                .strip_prefix("- ")
                .unwrap_or(trimmed)
                .strip_prefix("image:")
            else {
                continue;
            };
            let reference = rest
                .trim()
                .trim_matches(|character: char| character == '"' || character == '\'');
            let Some(name) = own_image(reference) else {
                continue;
            };
            let where_from = file
                .strip_prefix(root)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| file.display().to_string())
                .trim_start_matches('/')
                .to_string();
            if !found.iter().any(|(known, _)| *known == name) {
                found.push((name, where_from));
            }
        }
    }
    found.sort();
    found
}

/// The repository's images a workflow publishes.
pub fn published(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (_, text) in workflow_texts(root) {
        for (image, _) in image_names(&text) {
            let Some(name) = own_image(&image) else {
                continue;
            };
            if !found.contains(&name) {
                found.push(name);
            }
        }
    }
    found.sort();
    found
}

/// The Dockerfiles a workflow says it builds from. A build step points at a file; if the file is gone the build fails at the far
/// end of a queue instead of here.
pub fn published_dockerfiles(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (_, text) in workflow_texts(root) {
        for line in text.lines() {
            let Some(rest) = line.trim_start().strip_prefix("file:") else {
                continue;
            };
            let path = rest
                .trim()
                .trim_matches(|character: char| character == '"' || character == '\'')
                .trim_start_matches("./");
            if path.contains("Dockerfile") && !found.contains(&path.to_string()) {
                found.push(path.to_string());
            }
        }
    }
    found.sort();
    found
}

fn workflow_texts(root: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join(".github/workflows")) else {
        return found;
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".yml") || name.ends_with(".yaml"))
        .collect();
    names.sort();
    for name in names {
        if let Ok(text) = std::fs::read_to_string(root.join(".github/workflows").join(&name)) {
            found.push((name, text));
        }
    }
    found
}

/// Every image a workflow names, as `registry/namespace/name` with the tag taken off.
fn image_names(text: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let code = line.split('#').next().unwrap_or("");
        for token in code.split(|character: char| {
            character == ' ' || character == '"' || character == '\'' || character == ':'
        }) {
            if !token.contains("ghcr.io/") {
                continue;
            }
            // `images: ${{ env.NAMESPACE }}/calculation-api` is written in two pieces, so the suffix is read on its own below
            let without_tag = token
                .split('/')
                .take_while(|part| !part.contains('$'))
                .collect::<Vec<_>>()
                .join("/");
            if without_tag.is_empty() {
                continue;
            }
            found.push((without_tag.clone(), without_tag));
        }
        // the matrix names its image and the namespace separately; a line of the form `- image: name` completes it
        if let Some(rest) = line.trim_start().strip_prefix("- image:") {
            let name = rest.trim();
            if !name.is_empty() && !name.contains('/') {
                found.push((format!("{OWN_REGISTRY}{name}"), name.to_string()));
                found.push((name.to_string(), name.to_string()));
            }
        }
        if let Some(rest) = line.trim_start().strip_prefix("images:") {
            let value = rest.trim();
            if let Some((namespace, name)) = value.rsplit_once("}/") {
                if namespace.contains("$") && !name.contains('$') {
                    found.push((format!("{OWN_REGISTRY}{name}"), name.to_string()));
                }
            }
        }
    }
    found
}

/// `registry/namespace/name` for one of this repository's images, tag and digest removed; none for anybody else's.
fn own_image(reference: &str) -> Option<String> {
    let without_digest = reference.split('@').next().unwrap_or(reference);
    let without_tag = match without_digest.rsplit_once(':') {
        // a colon before the last slash is a port, not a tag
        Some((head, _)) if !head.contains('/') || head.rfind('/') > head.rfind(':') => head,
        _ => without_digest,
    };
    let name = without_tag.strip_prefix(OWN_REGISTRY)?;
    Some(format!("{OWN_REGISTRY}{name}"))
}

/// The manifests and compose variants that pull an image.
fn manifests(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = crate::repository::files(&root.join("infrastructure/kubernetes"));
    found.extend(crate::repository::files(
        &root.join("infrastructure/compose"),
    ));
    found
        .into_iter()
        .filter(|file| file.ends_with(".yaml") || file.ends_with(".yml"))
        .map(std::path::PathBuf::from)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
