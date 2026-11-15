#![forbid(unsafe_code)]

//! What a deploy actually rolls out.
//!
//! The images carry `:latest`, so `kubectl apply -k` on an unchanged Deployment spec starts no rollout at all: the pod that is
//! running keeps running the previous image until something kills it. Both deploy workflows answered that by restarting one
//! Deployment by name, `api-gateway`. Everything else the overlay ships - the engine, the sync server, the worker - was applied and
//! never rolled, release after release, and nothing anywhere said so, because a restart of the wrong thing looks exactly like a
//! restart of the right thing in a green log.
//!
//! The rule is therefore narrow and checkable: a rollout in a workflow may not name a Deployment. `scripts/deploy.sh` reads the set
//! from the cluster after the apply, so a Deployment added to an overlay rolls out because it exists.

use std::path::Path;

/// The one script allowed to name a rollout, because it derives the set instead of stating it.
const DEPLOY_SCRIPT: &str = "scripts/deploy.sh";

/// What is wrong: a workflow restarting a named Deployment, or a deploy that does not go through the script.
pub fn rollout_problems(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let script = root.join(DEPLOY_SCRIPT);
    if !script.is_file() {
        problems.push(format!(
            "{DEPLOY_SCRIPT} is gone, so nothing rolls a release out"
        ));
    } else if !reads_the_cluster(&script) {
        problems.push(format!(
            "{DEPLOY_SCRIPT} does not read the Deployment set from the cluster, so it is back to a list that goes stale"
        ));
    }

    let mut found_a_deploy = false;
    for (name, text) in workflows(root) {
        for line in text.lines() {
            let code = line.split('#').next().unwrap_or("").trim();
            if !code.contains("kubectl rollout") {
                continue;
            }
            if let Some(named) = named_deployment(code) {
                problems.push(format!(
                    "{name} restarts `{named}` by name, so every other Deployment the overlay ships keeps running the previous image; use {DEPLOY_SCRIPT}"
                ));
            }
        }
        if text.contains("kubectl apply -k") || text.contains(DEPLOY_SCRIPT) {
            found_a_deploy = true;
        }
        if text.contains("kubectl apply -k") && !text.contains(DEPLOY_SCRIPT) {
            problems.push(format!(
                "{name} applies an overlay without {DEPLOY_SCRIPT}, so a new image is applied and never rolled out"
            ));
        }
    }
    if !found_a_deploy {
        problems
            .push("no workflow applies an overlay, so this scan is reading nothing".to_string());
    }
    problems
}

/// The Deployment a `kubectl rollout restart` line names, if it names one.
fn named_deployment(line: &str) -> Option<String> {
    let after = line.split("rollout restart").nth(1)?;
    let words: Vec<&str> = after.split_whitespace().collect();
    let unquote = |word: &str| {
        word.trim_matches(|character: char| character == '"' || character == '\'')
            .to_string()
    };
    for (index, word) in words.iter().enumerate() {
        let bare = unquote(word);
        if let Some(name) = bare
            .strip_prefix("deployment/")
            .or_else(|| bare.strip_prefix("deploy/"))
        {
            return (!name.is_empty()).then(|| name.to_string());
        }
        if bare == "deployment" || bare == "deployments" || bare == "deploy" {
            // `deployment -n namespace name`: skip the flags, then the name is the next word, if there is one
            let mut next = index + 1;
            while next < words.len() && words[next].starts_with('-') {
                next += if words[next] == "-n" || words[next] == "--namespace" {
                    2
                } else {
                    1
                };
            }
            return words
                .get(next)
                .map(|word| unquote(word))
                .filter(|name| !name.is_empty());
        }
    }
    None
}

/// Does the script ask the cluster which Deployments exist?
fn reads_the_cluster(script: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(script) else {
        return false;
    };
    text.contains("kubectl get deployment") && text.contains("jsonpath")
}

fn workflows(root: &Path) -> Vec<(String, String)> {
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
