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
    assert!(
        !pinned.is_empty(),
        "the migration is not finished, so it is not empty"
    );
}

#[test]
fn the_first_migrated_feature_has_the_parts_it_actually_holds() {
    let features = flutter::features(repo());
    let authentication = features
        .iter()
        .find(|feature| feature.name == "authentication")
        .expect("authentication is the first feature moved");
    for part in ["presentation", "data", "models"] {
        assert!(
            authentication.parts.iter().any(|has| has == part),
            "authentication should hold {part}: {:?}",
            authentication.parts
        );
    }
    assert!(
        !authentication.parts.iter().any(|has| has == "application"),
        "a part with no file in it is not migrated, and is not claimed"
    );
}

#[test]
fn the_migration_is_real_progress_rather_than_a_rename() {
    // the exit criterion of P7 is that those folders hold nothing; until then this says how far in it is
    let left = flutter::leftovers(repo()).len();
    let all = flutter::dart_files(repo()).len();
    assert!(
        left < all,
        "some files have moved: {left} of {all} still in a layer folder"
    );
}
