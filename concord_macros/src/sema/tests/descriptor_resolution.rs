use super::helpers::{analyze_ok, endpoint_by_name, single_endpoint};
use crate::sema::{
    ApiOriginIr, EndpointOriginIr, OriginSchemeIr, RequestBodyDescriptorIr,
    ResponseFormatDescriptorIr,
};

#[test]
fn literal_https_origin_is_fixed_in_resolved_ir() {
    let api = analyze_ok(
        r#"
        client FixedHttps { base "https://example.com" }
        GET Ping path ["ping"] -> Json<String>
        "#,
    );
    let ApiOriginIr::FixedSingle(origin) = &api.descriptor.origin else {
        panic!("expected fixed API origin: {:?}", api.descriptor.origin);
    };
    assert_eq!(origin.scheme, OriginSchemeIr::Https);
    assert_eq!(origin.authority, "example.com");
    assert!(matches!(
        single_endpoint(&api).descriptor.origin,
        EndpointOriginIr::Fixed(_)
    ));
}

#[test]
fn literal_http_origin_is_fixed_in_resolved_ir() {
    let api = analyze_ok(
        r#"
        client FixedHttp { base "http://example.test" }
        GET Ping -> NoContent
        "#,
    );
    let ApiOriginIr::FixedSingle(origin) = &api.descriptor.origin else {
        panic!("expected fixed API origin");
    };
    assert_eq!(origin.scheme, OriginSchemeIr::Http);
    assert_eq!(origin.authority, "example.test");
}

#[test]
fn valid_ports_and_ip_authorities_are_fixed() {
    for (base, expected) in [
        ("https://example.com:8443", "example.com:8443"),
        ("http://192.0.2.1", "192.0.2.1"),
        ("https://[2001:db8::1]", "[2001:db8::1]"),
        ("https://[2001:db8::1]:9443", "[2001:db8::1]:9443"),
    ] {
        let api = analyze_ok(&format!(
            r#"
            client ValidAuthority {{ base "{base}" }}
            GET Ping -> Json<String>
            "#
        ));
        let ApiOriginIr::FixedSingle(origin) = &api.descriptor.origin else {
            panic!("expected `{base}` to be fixed: {:?}", api.descriptor.origin);
        };
        assert_eq!(origin.authority, expected);
    }
}

#[test]
fn runtime_host_fragment_is_dynamic_and_never_fixed() {
    let api = analyze_ok(
        r#"
        client DynamicHost {
            base "https://example.com"
            var region: String
        }
        scope regional {
            host [vars.region, "api"]
            GET Ping -> Json<String>
        }
        "#,
    );
    assert_eq!(api.descriptor.origin, ApiOriginIr::Dynamic);
    assert_eq!(
        single_endpoint(&api).descriptor.origin,
        EndpointOriginIr::Dynamic
    );
}

#[test]
fn unsafe_static_host_labels_are_never_fixed() {
    for label in [
        "user@evil",
        "bad/label",
        "bad label",
        "bad:443",
        "bad?query",
        "bad#fragment",
    ] {
        let api = analyze_ok(&format!(
            r#"
            client UnsafeLabel {{ base "https://example.com" }}
            scope unsafe_origin {{
                host ["{label}"]
                GET Ping -> Json<String>
            }}
            "#
        ));
        assert_eq!(
            api.descriptor.origin,
            ApiOriginIr::Dynamic,
            "unsafe label `{label}` granted fixed API eligibility"
        );
        assert_eq!(
            single_endpoint(&api).descriptor.origin,
            EndpointOriginIr::Dynamic
        );
    }
}

#[test]
fn malformed_base_ports_are_never_fixed() {
    for authority in [
        "example.com:",
        "example.com:not-a-port",
        "example.com:65536",
        "example.com:-1",
    ] {
        let api = analyze_ok(&format!(
            r#"
            client InvalidPort {{ base "https://{authority}" }}
            GET Ping -> Json<String>
            "#
        ));
        assert_eq!(api.descriptor.origin, ApiOriginIr::Dynamic);
        assert_eq!(
            single_endpoint(&api).descriptor.origin,
            EndpointOriginIr::Dynamic
        );
    }
}

#[test]
fn static_prefixes_cannot_be_joined_to_ip_authorities() {
    for base in ["https://192.0.2.1", "https://[2001:db8::1]"] {
        let api = analyze_ok(&format!(
            r#"
            client InvalidCombination {{ base "{base}" }}
            scope prefixed {{
                host ["api"]
                GET Ping -> Json<String>
            }}
            "#
        ));
        assert_eq!(api.descriptor.origin, ApiOriginIr::Dynamic);
        assert_eq!(
            single_endpoint(&api).descriptor.origin,
            EndpointOriginIr::Dynamic
        );
    }
}

#[test]
fn runtime_scheme_or_whole_origin_is_not_part_of_the_current_language() {
    let error = syn::parse_str::<crate::ast::RawApi>(
        r#"
        client RuntimeOrigin {
            var origin: String
            base vars.origin
        }
        GET Ping -> Json<String>
        "#,
    )
    .expect_err("the base grammar requires a URL literal");
    assert!(error.to_string().contains("single URL literal"));
}

#[test]
fn multiple_declared_static_authorities_are_multi_origin() {
    let api = analyze_ok(
        r#"
        client Multiple { base "https://example.com" }
        scope public {
            host ["api"]
            GET Ping -> Json<String>
        }
        scope assets {
            host ["cdn"]
            GET Logo -> Bytes
        }
        "#,
    );
    assert_eq!(api.descriptor.origin, ApiOriginIr::Multi);
}

#[test]
fn pagination_origin_capability_comes_from_resolved_host_bindings() {
    let api = analyze_ok(
        r#"
        client Pages { base "https://example.com" }
        scope tenant(cursor?: String) {
            host [cursor]
            GET Across
                paginate CursorPagination<String> { cursor = cursor }
                -> Json<Vec<String>>
        }
        GET Same(page: u64 = 1)
            path ["items"]
            paginate PagedPagination { page = page }
            -> Json<Vec<String>>
        "#,
    );
    assert!(
        endpoint_by_name(&api, "Across")
            .descriptor
            .pagination_can_change_origin
    );
    assert!(
        !endpoint_by_name(&api, "Same")
            .descriptor
            .pagination_can_change_origin
    );
}

#[test]
fn descriptor_io_categories_are_resolved_before_generation() {
    let api = analyze_ok(
        r#"
        client Io { base "https://example.com" }
        POST Create(body: Json<String>) -> Text<String>
        "#,
    );
    let descriptor = &single_endpoint(&api).descriptor;
    assert!(matches!(
        descriptor.request_body,
        RequestBodyDescriptorIr::Buffered { ref codec } if codec == "Json"
    ));
    assert!(matches!(
        descriptor.response_format,
        ResponseFormatDescriptorIr::Buffered { ref codec } if codec == "Text"
    ));
}
