#![forbid(unsafe_code)]

//! The estate configured a store it did not have. The engine read `NEO4J_URI=bolt://neo4j:7687`, the kustomization generated
//! `bolt://neo4j-service:7687` under the same ConfigMap name, no manifest declared a Neo4j at all, and the worker read
//! `neo4j-password` from a Secret declaring `NEO4J_PASSWORD`. Each file is plausible on its own; together they are a cluster that
//! starts, mounts its record routes, and fails at the first query. These tests pin the two comparisons that see it.

use std::path::{Path, PathBuf};

use xtask::manifests;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let mut directory = std::env::temp_dir();
    directory.push(format!("vimbai-store-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("infrastructure/kubernetes/base"))
        .expect("a scratch estate");
    directory
}

fn write(root: &Path, path: &str, contents: &str) {
    let file = root.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).expect("a directory for the file");
    }
    std::fs::write(file, contents).expect("a written file");
}

const SECRET: &str = "apiVersion: v1\nkind: Secret\nmetadata:\n  name: vimbai-secrets\ntype: Opaque\nstringData:\n  JWT_SECRET: \"x\"\n  VIMBAI_POSTGRES_URL: \"postgres://vimbai:x@postgres:5432/vimbai\"\n---\napiVersion: v1\nkind: Service\nmetadata:\n  name: postgres\nspec:\n  ports:\n    - port: 5432\n";

#[test]
fn the_estate_configures_a_store_it_provides() {
    let problems = manifests::store_problems(&repository());
    assert_eq!(
        problems,
        Vec::<String>::new(),
        "a store, or a Secret key, is named and not provided"
    );
}

#[test]
fn a_key_a_secret_does_not_declare_is_reported() {
    let root = scratch("key");
    write(&root, "infrastructure/kubernetes/base/secret.yaml", SECRET);
    write(
        &root,
        "infrastructure/kubernetes/base/worker.yaml",
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: worker\nspec:\n  template:\n    spec:\n      containers:\n        - name: worker\n          env:\n            - name: NEO4J_PASSWORD\n              valueFrom:\n                secretKeyRef:\n                  name: vimbai-secrets\n                  key: neo4j-password\n",
    );
    let problems = manifests::store_problems(&root);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("reads `neo4j-password`")),
        "{problems:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch estate goes");
}

#[test]
fn a_store_address_no_service_provides_is_reported() {
    let root = scratch("host");
    write(&root, "infrastructure/kubernetes/base/secret.yaml", SECRET);
    write(
        &root,
        "infrastructure/kubernetes/base/config.yaml",
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: vimbai-config\ndata:\n  NEO4J_URI: \"bolt://neo4j:7687\"\n",
    );
    let problems = manifests::store_problems(&root);
    assert!(
        problems.iter().any(|problem| problem.contains("`neo4j`")),
        "a store address naming a Service the estate does not serve has to be reported: {problems:?}"
    );

    // the same address, this time with the Service declared, is fine - the rule is parity, not a ban on a name
    write(
        &root,
        "infrastructure/kubernetes/base/neo4j.yaml",
        "apiVersion: v1\nkind: Service\nmetadata:\n  name: neo4j\nspec:\n  ports:\n    - port: 7687\n",
    );
    assert_eq!(
        manifests::store_problems(&root),
        Vec::<String>::new(),
        "an address the estate serves is not a defect"
    );

    std::fs::remove_dir_all(&root).expect("the scratch estate goes");
}
