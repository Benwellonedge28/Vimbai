#![forbid(unsafe_code)]
//! The P7 gate over the real app, rather than over a fixture.
//!
//! P7 moves 58 dart files out of five layer folders. There is no dart toolchain in CI, so a file move that leaves an import
//! pointing at nothing would ship silently. These tests are what notices.

use std::path::Path;

use xtask::flutter;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root, two levels above rust/xtask")
}

#[test]
fn nothing_is_added_to_an_old_layer_folder() {
    let added = flutter::regressions(repo());
    assert!(
        added.is_empty(),
        "a dart file belongs under lib/features/, not in a layer folder: {added:?}"
    );
}

#[test]
fn the_pinned_baseline_names_files_that_are_still_there() {
    let stale = flutter::retired_baseline_entries(repo());
    assert!(
        stale.is_empty(),
        "these moved, so the pinned list must shrink in the same commit: {stale:?}"
    );
}

#[test]
fn every_import_names_a_file_the_app_has() {
    let broken = flutter::dangling_imports(repo());
    assert!(broken.is_empty(), "{broken:?}");
}

#[test]
fn the_baseline_is_evidence_rather_than_a_list_of_names() {
    let pinned = flutter::baseline(repo());
    let left = flutter::leftovers(repo());
    assert_eq!(
        pinned.len(),
        left.len(),
        "the pinned list and the layer folders must agree, or the list stops meaning anything"
    );
}

/// The exit criterion of P7, as a test: those five folders hold no dart file at all.
#[test]
fn no_dart_file_is_left_in_a_layer_folder() {
    let left = flutter::leftovers(repo());
    assert!(
        left.is_empty(),
        "P7 is finished when this is empty: {left:?}"
    );
    assert!(
        flutter::baseline(repo()).is_empty(),
        "nothing needs pinning once nothing is left behind"
    );
}

#[test]
fn each_feature_holds_the_parts_it_actually_has() {
    let features = flutter::features(repo());
    let names: Vec<&str> = features
        .iter()
        .map(|feature| feature.name.as_str())
        .collect();
    for expected in [
        "accounting",
        "authentication",
        "banking",
        "books",
        "budgeting",
        "group_savings",
        "household_finance",
        "multimodal",
        "npo",
        "personal_finance",
        "reporting",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} should have its own folder: {names:?}"
        );
    }
    for feature in &features {
        assert!(
            !feature.parts.is_empty(),
            "{} is a folder with no file in it, which git would not even keep",
            feature.name
        );
        assert!(
            feature.parts.iter().any(|part| part == "presentation"),
            "{} has no screen, so it is not a feature yet: {:?}",
            feature.name,
            feature.parts
        );
    }
}

#[test]
fn every_file_moved_and_none_was_lost() {
    // the count before P7 started, so a move that dropped a file is a failure rather than a smaller tree
    let all = flutter::dart_files(repo()).len();
    assert!(
        all >= 58,
        "the app had 58 dart files when P7 started: {all}"
    );
}
