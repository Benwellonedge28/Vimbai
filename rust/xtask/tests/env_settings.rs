#![forbid(unsafe_code)]

//! A variable a manifest sets is a variable the code reads.
//!
//! Three of these were found by writing this rule down: seven service URLs configured for a gateway that routes from the
//! catalogue, a pod declaring a container port while the server it runs listened somewhere else, and an estate handing out
//! `OTEL_EXPORTER_OTLP_ENDPOINT` with no exporter linked into any binary. Each one read, in its own file, as configuration.

use std::path::Path;

use xtask::settings;

fn a_directory(name: &str) -> std::path::PathBuf {
    let directory =
        std::env::temp_dir().join(format!("vimbai-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("infrastructure/kubernetes/base")).expect("a directory");
    std::fs::create_dir_all(directory.join("rust/apps/thing/src")).expect("a directory");
    directory
}

#[test]
fn a_container_environment_is_read_both_ways_it_is_spelled() {
    let text = "\
spec:
  containers:
  - name: engine
    env:
    - name: VIMBAI_API_ADDR
      value: \"0.0.0.0:8099\"
    - name: FROM_A_SECRET
      valueFrom:
        secretKeyRef:
          name: vimbai-secrets
          key: JWT_SECRET
    ports:
    - containerPort: 8099
";
    assert_eq!(
        settings::env_names(text),
        vec!["VIMBAI_API_ADDR".to_string(), "FROM_A_SECRET".to_string()]
    );

    let compose = "\
services:
  engine:
    environment:
      VIMBAI_STORE_FILE: /data/store.json
      JWT_SECRET: ${JWT_SECRET:-dev}
";
    assert_eq!(
        settings::env_names(compose),
        vec!["VIMBAI_STORE_FILE".to_string(), "JWT_SECRET".to_string()]
    );
}

#[test]
fn a_block_ends_where_its_indentation_ends() {
    // `ports:` follows `env:`, and its entries are not variables
    let text = "\
    env:
    - name: ONE
    - name: TWO
    ports:
    - containerPort: 8099
      name: http
";
    assert_eq!(
        settings::env_names(text),
        vec!["ONE".to_string(), "TWO".to_string()]
    );
}

#[test]
fn a_quoted_name_is_read_from_any_of_the_three_languages() {
    let text = r#"
let address = std::env::var("VIMBAI_API_ADDR").unwrap_or_default();
let secret = getEnv("RATE_LIMIT_ENABLED", "true");
const url = String.fromEnvironment('VIMBAI_GATEWAY_URL');
// a comment naming NOT_A_READER is not a reader
"#;
    let names = settings::quoted_names(text);
    assert!(names.contains(&"VIMBAI_API_ADDR".to_string()));
    assert!(names.contains(&"RATE_LIMIT_ENABLED".to_string()));
    assert!(names.contains(&"VIMBAI_GATEWAY_URL".to_string()));
    assert!(!names.contains(&"NOT_A_READER".to_string()));
}

#[test]
fn a_name_that_is_not_a_variable_is_not_a_variable() {
    assert!(settings::is_variable_name("JWT_SECRET"));
    assert!(settings::is_variable_name("RPS"));
    assert!(!settings::is_variable_name("secret"));
    assert!(!settings::is_variable_name("Secret"));
    assert!(!settings::is_variable_name("A"));
    assert!(!settings::is_variable_name("X"));
    assert!(!settings::is_variable_name("with-hyphen"));
}

#[test]
fn a_configured_name_nothing_reads_is_reported_and_a_read_one_is_not() {
    let root = a_directory("pair");
    std::fs::write(
        root.join("infrastructure/kubernetes/base/deployment.yaml"),
        "spec:\n  env:\n  - name: READ_SOMEWHERE\n  - name: NOBODY_READS_THIS\n",
    )
    .expect("a manifest");
    std::fs::write(
        root.join("rust/apps/thing/src/main.rs"),
        "fn main() { let _ = std::env::var(\"READ_SOMEWHERE\"); }\n",
    )
    .expect("a source file");

    let problems = settings::problems(&root);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(problems[0].contains("NOBODY_READS_THIS"), "{problems:#?}");
    assert!(!problems[0].contains("READ_SOMEWHERE"), "{problems:#?}");
}

#[test]
fn a_scan_that_reads_nothing_says_so_instead_of_passing() {
    let root = a_directory("empty");
    let problems = settings::problems(&root);
    assert!(!problems.is_empty(), "an empty scan has to fail, not pass");
}

#[test]
fn this_estate_sets_nothing_nothing_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf();
    let mut problems = settings::problems(&root);
    problems.extend(settings::excuse_problems(&root));
    assert!(problems.is_empty(), "{problems:#?}");

    // the scan has to have found something to be worth trusting
    assert!(settings::configured(&root).len() >= 10);
    assert!(settings::named_in_source(&root).len() >= 100);
}

#[test]
fn every_excused_name_says_whose_it_is_and_is_still_configured() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf();
    let configured = settings::configured(&root);
    assert!(
        !settings::EXTERNAL.is_empty(),
        "an empty excuse list is not a rule"
    );
    for (name, reason) in settings::EXTERNAL {
        assert!(
            reason.contains("image"),
            "`{name}` is excused without saying which image reads it: {reason}"
        );
        assert!(
            configured.contains_key(*name),
            "`{name}` is excused but nothing sets it any more"
        );
    }
}
