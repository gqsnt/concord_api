use concord_core::__private::{
    DebugLevel, GENERATED_CONTRACT_VERSION, HeadBuilder, Method, RequestMeta, RequestOptions, Url,
};
use concord_core::contracts::ErrorCategory;

#[test]
fn generated_contract_version_matches_generic_specification_capability() {
    assert_eq!(GENERATED_CONTRACT_VERSION, 34);
}

#[test]
fn structured_head_builder_encodes_values_only_when_prepared() {
    let base = Url::parse("https://example.test/api/").expect("base URL");
    let mut builder = HeadBuilder::new(
        &base,
        RequestMeta::new("proof_api.resources.by_id", Method::GET, None),
        &RequestOptions::new(),
        DebugLevel::None,
        None,
    );
    builder.push_static("resources").expect("static segment");
    builder.push_value("item tail").expect("dynamic segment");
    let head = builder.finish().expect("bounded head");
    assert_eq!(head.endpoint(), "proof_api.resources.by_id");
    assert_eq!(head.method(), &Method::GET);
    assert_eq!(
        head.logical_url().as_str(),
        "https://example.test/api/resources/item%20tail"
    );
    assert!(head.public_headers().is_empty());
}

#[test]
fn structured_head_builder_enforces_one_non_dot_segment_per_atom() {
    fn dynamic(value: &str) -> Result<String, ErrorCategory> {
        let base = Url::parse("https://example.test/api/").expect("base URL");
        let mut builder = HeadBuilder::new(
            &base,
            RequestMeta::new("proof_api.resources.by_id", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        );
        builder
            .push_value(value)
            .map_err(|error| error.category())?;
        builder
            .finish()
            .map(|head| head.logical_url().to_string())
            .map_err(|error| error.category())
    }

    assert_eq!(dynamic("slash/value"), Err(ErrorCategory::Configuration));
    assert_eq!(
        dynamic(r"backslash\value"),
        Err(ErrorCategory::Configuration)
    );
    // PATH §5.3: percent signs are ordinary data, encoded exactly once; a
    // user-provided `%2F` is neither decoded into a slash nor rejected.
    assert_eq!(
        dynamic("already%2Fencoded").expect("percent data remains ordinary data"),
        "https://example.test/api/already%252Fencoded"
    );
    assert_eq!(
        dynamic("50%done").expect("a non-escape percent remains ordinary data"),
        "https://example.test/api/50%25done"
    );
    for invalid in ["", ".", ".."] {
        assert_eq!(dynamic(invalid), Err(ErrorCategory::Configuration));
    }

    let base = Url::parse("https://example.test/api/").expect("base URL");
    for invalid in ["two/segments", r"two\segments"] {
        let mut builder = HeadBuilder::new(
            &base,
            RequestMeta::new("proof_api.resources.by_id", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        );
        let error = builder
            .push_static(invalid)
            .expect_err("static DSL atoms cannot contain separators");
        assert_eq!(error.category(), ErrorCategory::Configuration);
    }
}
