#![forbid(unsafe_code)]

//! Both deploy workflows restarted `deployment/api-gateway` and nothing else, so on every release the engine, the sync server and the
//! worker were applied with a new image and never rolled: with `:latest` a spec that has not changed starts no rollout, so those pods
//! kept serving the previous image. A green log cannot tell the two apart. These tests pin the rule that makes it visible.

use std::path::{Path, PathBuf};

use xtask::deploy;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let mut directory = std::env::temp_dir();
    directory.push(format!("vimbai-deploy-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

fn write(root: &Path, path: &str, contents: &str) {
    let file = root.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).expect("a directory for the file");
    }
    std::fs::write(file, contents).expect("a written file");
}

const ROLLING_SCRIPT: &str = "#!/usr/bin/env bash\nkubectl get deployment -n vimbai -o jsonpath='{.items[*].metadata.name}'\n";

#[test]
fn a_deploy_rolls_out_every_deployment_it_applies() {
    let root = repository();
    assert_eq!(
        deploy::rollout_problems(&root),
        Vec::<String>::new(),
        "a deploy restarts part of the release, or applies without restarting at all"
    );

    // and the script it goes through does read the set from the cluster rather than stating it
    let script =
        std::fs::read_to_string(root.join("scripts/deploy.sh")).expect("the deploy script");
    assert!(
        script.contains("kubectl get deployment"),
        "the script names deployments instead of asking"
    );
    let code: String = script
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<&str>>()
        .join("\n");
    assert!(
        !code.contains("api-gateway"),
        "the script names a deployment in code, so the list has started going stale again"
    );
}

#[test]
fn restarting_one_deployment_by_name_is_reported() {
    let root = scratch("named");
    write(&root, "scripts/deploy.sh", ROLLING_SCRIPT);
    write(
        &root,
        ".github/workflows/cd-production.yml",
        "jobs:\n  deploy:\n    steps:\n      - run: |\n          kubectl apply -k infrastructure/kubernetes/overlays/production\n          ./scripts/deploy.sh production\n          kubectl rollout restart deployment -n vimbai api-gateway\n",
    );
    let problems = deploy::rollout_problems(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("`api-gateway` by name")),
        "{problems:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn applying_an_overlay_without_the_script_is_reported() {
    let root = scratch("no-script");
    write(&root, "scripts/deploy.sh", ROLLING_SCRIPT);
    write(
        &root,
        ".github/workflows/cd-staging.yml",
        "jobs:\n  deploy:\n    steps:\n      - run: kubectl apply -k infrastructure/kubernetes/overlays/staging\n",
    );
    let problems = deploy::rollout_problems(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("applied and never rolled out")),
        "{problems:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn a_rollout_of_the_whole_namespace_and_a_missing_script_are_read_correctly() {
    let root = scratch("whole-namespace");
    write(&root, "scripts/deploy.sh", ROLLING_SCRIPT);
    write(
        &root,
        ".github/workflows/cd-staging.yml",
        "jobs:\n  deploy:\n    steps:\n      - run: |\n          ./scripts/deploy.sh staging\n          kubectl rollout restart deployment -n vimbai\n          kubectl rollout status deployment -n vimbai --timeout=300s\n",
    );
    assert_eq!(
        deploy::rollout_problems(&root),
        Vec::<String>::new(),
        "restarting every deployment in the namespace names none of them"
    );

    // a script that lists its deployments rather than asking for them is the same defect one layer down
    write(
        &root,
        "scripts/deploy.sh",
        "#!/usr/bin/env bash\nkubectl rollout restart deployment -n vimbai api-gateway\n",
    );
    let problems = deploy::rollout_problems(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("does not read the Deployment set")),
        "{problems:?}"
    );

    std::fs::remove_file(root.join("scripts/deploy.sh")).expect("the script goes");
    let problems = deploy::rollout_problems(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("is gone, so nothing rolls a release out")),
        "{problems:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}
