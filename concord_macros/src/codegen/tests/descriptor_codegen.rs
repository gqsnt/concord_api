use super::helpers::*;
use quote::quote;

#[test]
fn generated_api_and_endpoints_use_the_current_private_contract() {
    let out = expanded(quote! {
        client DescriptorApi {
            base "https://example.com"
            secret token: String
            credential session = bearer(secret.token)
        }
        GET List(page: u64 = 1)
            auth bearer session
            paginate PagedPagination { page = page }
            -> Json<Vec<String>>
        POST Create(body: Text<String>) -> NoContent
    });

    assert_eq!(out.matches("assert_generated_contract").count(), 1);
    assert_eq!(out.matches("GENERATED_CONTRACT").count(), 1);
    assert_eq!(out.matches("ReqwestNativeGeneratedContract").count(), 1);
    assert_contains_all(
        &out,
        &[
            "pub static API_DESCRIPTOR : :: concord_core :: __private :: GeneratedApiDescriptor",
            "GeneratedApiDescriptor :: new (\"DescriptorApi\"",
            "GeneratedApiOriginDescriptor :: FixedSingleOrigin",
            "GeneratedEndpointDescriptor :: new (\"List\"",
            "GeneratedEndpointDescriptor :: new (\"Create\"",
            "HttpMethod :: Get",
            "HttpMethod :: Post",
            "RequestBodyDescriptor :: None",
            "RequestBodyDescriptor :: Buffered { codec : \"Text\" }",
            "ResponseFormatDescriptor :: Buffered { codec : \"Json\" }",
            "ResponseFormatDescriptor :: NoContent",
            "AuthRequirementDescriptor :: new (\"session\"",
            "PaginationDescriptor :: new (false",
        ],
    );

    assert!(out.contains("ReqwestNativeGeneratedContract"));
}

#[test]
fn descriptor_definitions_exclude_execution_internals_and_match_runtime_facts() {
    let out = expanded(quote! {
        client DriftApi { base "http://example.test" }
        PUT Save(page: u64 = 1)
            paginate PagedPagination { page = 1 }
            -> Json<Vec<String>>
    });
    let start = out.find("staticEP").expect("endpoint descriptor static");
    let end = out[start..]
        .find("pubstructEp")
        .map(|offset| start + offset)
        .expect("endpoint type after descriptor");
    let descriptor = &out[start..end];

    for forbidden in [
        "Transport",
        "Reqwest",
        "RetryContext",
        "DynBody",
        "poll_",
        "CredentialProvider",
    ] {
        assert!(
            !descriptor.contains(forbidden),
            "descriptor contained {forbidden}"
        );
    }
    assert!(descriptor.contains("HttpMethod::Put"));
    assert!(descriptor.contains("RequestBodyDescriptor::None"));
    assert!(descriptor.contains("PaginationDescriptor::new(false)"));
    assert!(out.contains("method:::http::Method::PUT"));
    assert!(out.contains("let__pagination_plan=true"));
}

#[test]
fn dynamic_and_multi_origin_descriptors_are_emitted_from_ir() {
    let dynamic = expanded(quote! {
        client Dynamic { base "https://example.com" var tenant: String }
        scope tenant { host [vars.tenant] GET Ping -> Json<String> }
    });
    assert!(dynamic.contains("ApiOriginDescriptor::DynamicOrigin"));
    assert!(dynamic.contains("EndpointOriginDescriptor::Dynamic"));

    let multiple = expanded(quote! {
        client Multiple { base "https://example.com" }
        scope one { host ["one"] GET Ping -> Json<String> }
        scope two { host ["two"] GET Pong -> Json<String> }
    });
    assert!(multiple.contains("ApiOriginDescriptor::MultiOrigin"));
}

#[test]
fn unsafe_static_authority_is_not_embedded_in_a_descriptor() {
    let out = expanded(quote! {
        client Unsafe { base "https://example.com" }
        scope unsafe_origin {
            host ["user@evil"]
            GET Ping -> Json<String>
        }
    });
    let start = out.find("staticEP").expect("endpoint descriptor static");
    let end = out[start..]
        .find("pubstructEp")
        .map(|offset| start + offset)
        .expect("endpoint type after descriptor");
    let descriptor = &out[start..end];

    assert!(descriptor.contains("EndpointOriginDescriptor::Dynamic"));
    assert!(!descriptor.contains("user@evil"));
    assert!(out.contains("ApiOriginDescriptor::DynamicOrigin"));
}
