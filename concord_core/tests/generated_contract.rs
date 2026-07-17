use concord_core::__private::{
    DebugLevel, GENERATED_CONTRACT_VERSION, HeadBuilder, Method, RequestMeta, RequestOptions, Url,
};

#[test]
fn generated_contract_version_is_two() {
    assert_eq!(GENERATED_CONTRACT_VERSION, 2);
}

#[test]
fn structured_head_builder_encodes_values_only_when_prepared() {
    let base = Url::parse("https://example.test/api/").expect("base URL");
    let mut builder = HeadBuilder::new(
        &base,
        RequestMeta::new("ProofApi::resources::ById", Method::GET, None),
        &RequestOptions::new(),
        DebugLevel::None,
    );
    builder.push_static("resources").expect("static segment");
    builder
        .push_value("item/omega%2Ftail")
        .expect("dynamic segment");
    let head = builder.finish();
    assert_eq!(head.endpoint(), "ProofApi::resources::ById");
    assert_eq!(head.method(), &Method::GET);
    assert_eq!(
        head.logical_url().as_str(),
        "https://example.test/api/resources/item%2Fomega%252Ftail"
    );
    assert!(head.public_headers().is_empty());
}
