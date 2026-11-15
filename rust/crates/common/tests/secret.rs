#![forbid(unsafe_code)]

//! The engine and the sync server sign and verify the same tokens. They resolved the secret separately - the sync server from a
//! file beside its data, generated when absent - so a deployment that supplied one had a sync server signing with a secret nothing
//! else knew, and every token minted on one side was refused on the other. The rule now lives here, once, and is read by both.

#[test]
fn the_engine_and_the_sync_server_read_one_secret() {
    let supplied = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        }
    };

    assert_eq!(
        vimbai_common::secret_from(supplied(&[("VIMBAI_JWT_SECRET", "one")])),
        Some("one".to_string())
    );
    // a deployment that set the short name is read too: the engine's identity surface has always accepted it
    assert_eq!(
        vimbai_common::secret_from(supplied(&[("JWT_SECRET", "two")])),
        Some("two".to_string())
    );
    // the prefixed name wins when both are set, so the two surfaces cannot disagree
    assert_eq!(
        vimbai_common::secret_from(supplied(&[
            ("JWT_SECRET", "two"),
            ("VIMBAI_JWT_SECRET", "one")
        ])),
        Some("one".to_string())
    );
    // a variable set to nothing is not a secret: falling back to a written one is better than signing with an empty key
    assert_eq!(
        vimbai_common::secret_from(supplied(&[("JWT_SECRET", "   ")])),
        None
    );
    assert_eq!(vimbai_common::secret_from(supplied(&[])), None);
}

#[test]
fn a_deployment_that_supplies_a_secret_is_what_the_process_reads() {
    // the real lookup, against a variable this test owns so it cannot race another one
    std::env::set_var("VIMBAI_JWT_SECRET", "deployment-secret");
    assert_eq!(
        vimbai_common::secret_from_environment(),
        Some("deployment-secret".to_string())
    );
    std::env::remove_var("VIMBAI_JWT_SECRET");
}
