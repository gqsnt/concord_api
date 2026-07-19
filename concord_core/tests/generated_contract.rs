use concord_core::__private::{
    DebugLevel, GENERATED_CONTRACT_VERSION, HeadBuilder, Method, RequestMeta, RequestOptions, Url,
};
use concord_core::advanced::ErrorCategory;

#[test]
fn generated_contract_version_is_ten() {
    assert_eq!(GENERATED_CONTRACT_VERSION, 10);
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
    builder.push_value("item%2Ftail").expect("dynamic segment");
    let head = builder.finish();
    assert_eq!(head.endpoint(), "ProofApi::resources::ById");
    assert_eq!(head.method(), &Method::GET);
    assert_eq!(
        head.logical_url().as_str(),
        "https://example.test/api/resources/item%252Ftail"
    );
    assert!(head.public_headers().is_empty());
}

#[test]
fn structured_head_builder_enforces_one_non_dot_segment_per_atom() {
    fn dynamic(value: &str) -> Result<String, ErrorCategory> {
        let base = Url::parse("https://example.test/api/").expect("base URL");
        let mut builder = HeadBuilder::new(
            &base,
            RequestMeta::new("ProofApi::resources::ById", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
        );
        builder
            .push_value(value)
            .map_err(|error| error.category())?;
        Ok(builder.finish().logical_url().to_string())
    }

    assert_eq!(dynamic("slash/value"), Err(ErrorCategory::Configuration));
    assert_eq!(
        dynamic(r"backslash\value"),
        Err(ErrorCategory::Configuration)
    );
    assert_eq!(
        dynamic("already%2Fencoded").expect("percent is encoded once by this boundary"),
        "https://example.test/api/already%252Fencoded"
    );
    for invalid in ["", ".", ".."] {
        assert_eq!(dynamic(invalid), Err(ErrorCategory::Configuration));
    }

    let base = Url::parse("https://example.test/api/").expect("base URL");
    for invalid in ["two/segments", r"two\segments"] {
        let mut builder = HeadBuilder::new(
            &base,
            RequestMeta::new("ProofApi::resources::ById", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
        );
        let error = builder
            .push_static(invalid)
            .expect_err("static DSL atoms cannot contain separators");
        assert_eq!(error.category(), ErrorCategory::Configuration);
    }
}
