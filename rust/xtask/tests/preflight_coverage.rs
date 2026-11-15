#![forbid(unsafe_code)]

//! A clean preflight has to mean a green pipeline, which it did not: `ci-main.yml` has run `dart format --set-exit-if-changed` and
//! `flutter analyze` since the app job was written, and `scripts/preflight.sh` ran `flutter test` alone. An unformatted or
//! unanalysed change passed preflight and failed the pipeline, and nothing compared the two lists.

use std::path::{Path, PathBuf};

use xtask::preflight;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let mut directory = std::env::temp_dir();
    directory.push(format!("vimbai-preflight-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join(".github/workflows")).expect("a workflows directory");
    std::fs::create_dir_all(directory.join("scripts")).expect("a scripts directory");
    directory
}

#[test]
fn preflight_runs_every_check_the_pipelines_run() {
    let root = repository();
    assert_eq!(
        preflight::checks_missing_from_preflight(&root),
        Vec::<String>::new(),
        "a pipeline runs a check preflight does not, so a clean preflight can still fail there"
    );

    // non-vacuous: the checks that were missing are now named, and the toolchains are all represented
    let runs = preflight::checks_in_preflight(&root);
    for expected in [
        "dart format",
        "flutter analyze",
        "flutter test",
        "cargo clippy",
        "go vet",
    ] {
        assert!(
            runs.contains(&expected.to_string()),
            "preflight does not run {expected}: {runs:?}"
        );
    }
}

#[test]
fn a_check_only_the_pipeline_runs_is_reported() {
    let root = scratch("missing");
    std::fs::write(
        root.join(".github/workflows/ci-main.yml"),
        "jobs:\n  app:\n    steps:\n      - run: flutter pub get\n      - run: dart format --output=none --set-exit-if-changed lib test\n",
    )
    .expect("a workflow");
    std::fs::write(
        root.join("scripts/preflight.sh"),
        "#!/bin/bash\n(cd apps/flutter && flutter pub get >/dev/null) || fail\n",
    )
    .expect("a preflight that prepares the toolchain but judges nothing");

    let missing = preflight::checks_missing_from_preflight(&root);
    assert_eq!(missing.len(), 1, "one check is missing: {missing:?}");
    assert!(missing[0].contains("dart format"));
    assert!(
        !missing[0].contains("pub get"),
        "preparing a toolchain is not a verdict, and demanding it of preflight is noise"
    );

    std::fs::write(
        root.join("scripts/preflight.sh"),
        "#!/bin/bash\n(cd apps/flutter && dart format --output=none --set-exit-if-changed lib test) || fail\n",
    )
    .expect("a preflight that runs the check");
    assert_eq!(
        preflight::checks_missing_from_preflight(&root),
        Vec::<String>::new(),
        "a check both run is covered"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn a_preflight_that_checks_nothing_is_not_called_clean() {
    let root = scratch("empty");
    std::fs::write(
        root.join(".github/workflows/ci-main.yml"),
        "jobs:\n  rust:\n    steps:\n      - run: cargo clippy --workspace -- -D warnings\n",
    )
    .expect("a workflow");
    std::fs::write(
        root.join("scripts/preflight.sh"),
        "#!/bin/bash\necho nothing\n",
    )
    .expect("a preflight with no checks");

    assert_eq!(
        preflight::checks_missing_from_preflight(&root),
        vec![
            "ci-main.yml runs `cargo clippy` and scripts/preflight.sh does not, so a clean preflight can still fail the pipeline"
                .to_string()
        ]
    );

    std::fs::write(
        root.join("scripts/preflight.sh"),
        "#!/bin/bash\necho nothing\n",
    )
    .expect("a preflight with no checks");
    std::fs::remove_file(root.join(".github/workflows/ci-main.yml")).expect("the workflow goes");
    assert_eq!(
        preflight::checks_missing_from_preflight(&root),
        vec!["no check was found in the pipelines or in scripts/preflight.sh".to_string()],
        "a script that checks nothing must not read as covering everything"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}
