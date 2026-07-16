use concord_core::__private::{GENERATED_CONTRACT_VERSION, RequestHead};

#[test]
fn generated_contract_version_is_one() {
    assert_eq!(GENERATED_CONTRACT_VERSION, 1);
}

#[test]
fn request_head_preserves_every_ordered_dimension() {
    let head = RequestHead::new(
        "ProofApi::resources::ById",
        "GET",
        "https://example.test/scopes/scope-alpha/resources/item-omega".to_owned(),
        vec![("first".to_owned(), "1".to_owned())],
        vec![("X-First".to_owned(), "one".to_owned())],
        vec![
            ("account", "scope-alpha".to_owned()),
            ("region", "region-beta".to_owned()),
        ],
    );

    assert_eq!(head.endpoint(), "ProofApi::resources::ById");
    assert_eq!(head.method(), "GET");
    assert_eq!(
        head.logical_url(),
        "https://example.test/scopes/scope-alpha/resources/item-omega"
    );
    assert_eq!(head.query(), &[("first".to_owned(), "1".to_owned())]);
    assert_eq!(head.headers(), &[("X-First".to_owned(), "one".to_owned())]);
    assert_eq!(
        head.scope_snapshot(),
        &[
            ("account", "scope-alpha".to_owned()),
            ("region", "region-beta".to_owned()),
        ]
    );
}
