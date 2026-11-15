#![forbid(unsafe_code)]

//! `docker-publish.yml` spent its life skipping every matrix entry: it looked for a Dockerfile inside a changed top-level directory,
//! which is the shape the Python microservices had and this repository does not, so the job was green and published nothing while
//! four Deployments asked ghcr.io for images nobody had pushed. These tests pin the comparison that would have caught it.

use std::path::{Path, PathBuf};

use xtask::images;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let mut directory = std::env::temp_dir();
    directory.push(format!("vimbai-images-{}-{name}", std::process::id()));
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

#[test]
fn every_image_a_manifest_pulls_is_one_a_workflow_publishes() {
    let root = repository();
    assert_eq!(
        images::unpublished_images(&root),
        Vec::<String>::new(),
        "the cluster pulls an image nothing builds, or builds one nothing runs"
    );

    let pulled: Vec<String> = images::pulled(&root)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for expected in ["calculation-api", "sync", "worker", "api-gateway"] {
        assert!(
            pulled.iter().any(|name| name.ends_with(expected)),
            "the manifests no longer pull {expected}, so this gate is not reading what it thinks: {pulled:?}"
        );
    }

    let published = images::published(&root);
    assert_eq!(
        published.len(),
        pulled.len(),
        "the manifest and the publish matrix disagree: {published:?} against {pulled:?}"
    );
    for dockerfile in images::published_dockerfiles(&root) {
        assert!(
            dockerfile.starts_with("infrastructure/docker/"),
            "{dockerfile} is not one of the shared Dockerfiles"
        );
    }
}

#[test]
fn an_image_nothing_publishes_is_reported() {
    let root = scratch("unpublished");
    write(
        &root,
        "infrastructure/kubernetes/base/engine.yaml",
        "spec:\n  containers:\n    - image: ghcr.io/benwellonedge28/vimbai/engine:latest\n",
    );
    write(
        &root,
        ".github/workflows/docker-publish.yml",
        "jobs:\n  build:\n    steps:\n      - uses: docker/build-push-action@v6\n        with:\n          file: infrastructure/docker/Dockerfile.rust\n",
    );
    // the workflow names no image at all, so nothing is published
    let problems = images::unpublished_images(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("no workflow publishes an image")),
        "{problems:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn an_image_nothing_runs_is_reported_too() {
    let root = scratch("unused");
    write(
        &root,
        "infrastructure/kubernetes/base/engine.yaml",
        "spec:\n  containers:\n    - image: ghcr.io/benwellonedge28/vimbai/calculation-api:latest\n",
    );
    write(
        &root,
        "infrastructure/docker/Dockerfile.rust",
        "FROM scratch\n",
    );
    write(
        &root,
        ".github/workflows/docker-publish.yml",
        "jobs:\n  build:\n    steps:\n      - with:\n          images: ${{ env.NAMESPACE }}/calculation-api\n      - uses: docker/build-push-action@v6\n        with:\n          file: infrastructure/docker/Dockerfile.rust\n",
    );
    assert_eq!(
        images::unpublished_images(&root),
        Vec::<String>::new(),
        "both sides line up: one image pulled, one published, one Dockerfile that is there"
    );

    // publishing an image nobody pulls spends a build on nothing
    write(
        &root,
        ".github/workflows/docker-publish.yml",
        "jobs:\n  build:\n    steps:\n      - with:\n          images: ${{ env.NAMESPACE }}/ghost\n      - uses: docker/build-push-action@v6\n        with:\n          file: infrastructure/docker/Dockerfile.rust\n",
    );
    let problems = images::unpublished_images(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("nothing pulls it")),
        "{problems:?}"
    );
    assert!(
        problems.iter().any(|problem| problem.contains("pulls ghcr.io/benwellonedge28/vimbai/calculation-api")),
        "the manifest still asks for an image nothing publishes: {problems:?}"
    );

    // a build step naming a Dockerfile that is not there fails at the far end of a queue, not here
    write(
        &root,
        ".github/workflows/docker-publish.yml",
        "jobs:\n  build:\n    steps:\n      - with:\n          images: ${{ env.NAMESPACE }}/calculation-api\n      - uses: docker/build-push-action@v6\n        with:\n          file: infrastructure/docker/Dockerfile.gone\n",
    );
    let problems = images::unpublished_images(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("Dockerfile.gone")),
        "a missing Dockerfile has to be reported: {problems:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn somebody_elses_image_is_not_this_repositorys_business() {
    let root = scratch("third-party");
    write(
        &root,
        "infrastructure/kubernetes/base/store.yaml",
        "spec:\n  containers:\n    - image: neo4j:5-community\n    - image: ghcr.io/benwellonedge28/vimbai/sync:latest\n",
    );
    write(
        &root,
        ".github/workflows/docker-publish.yml",
        "jobs:\n  build:\n    steps:\n      - with:\n          images: ${{ env.NAMESPACE }}/sync\n      - uses: docker/build-push-action@v6\n        with:\n          file: infrastructure/docker/Dockerfile.rust\n",
    );
    write(
        &root,
        "infrastructure/docker/Dockerfile.rust",
        "FROM scratch\n",
    );
    let pulled: Vec<String> = images::pulled(&root)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        pulled.len(),
        1,
        "only the repository's own image counts: {pulled:?}"
    );
    assert_eq!(images::unpublished_images(&root), Vec::<String>::new());

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}
