use concord_macros::api;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord_core::__private::{
    DebugLevel, EndpointSpec, HeadBuilder, PreparedHead, RequestOptions, Url,
};

struct MinimalValue(&'static str);

impl Display for MinimalValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

struct CountingValue {
    text: &'static str,
    calls: Arc<AtomicUsize>,
}

impl Display for CountingValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.calls.fetch_add(1, Ordering::SeqCst);
        formatter.write_str(self.text)
    }
}

fn prepared_head<E>(endpoint: &E, base: &str) -> PreparedHead
where
    E: EndpointSpec,
{
    let base = Url::parse(base).expect("test base URL");
    let builder = HeadBuilder::new(
        &base,
        endpoint.request_meta(),
        &RequestOptions::new(),
        DebugLevel::None,
    );
    endpoint
        .prepare_head_and_body_metadata(builder)
        .expect("generated head preparation")
        .0
}

fn assert_same_head(first: &PreparedHead, second: &PreparedHead) {
    assert_eq!(first.endpoint(), second.endpoint());
    assert_eq!(first.method(), second.method());
    assert_eq!(first.logical_url(), second.logical_url());
    assert_eq!(first.safe_url(), second.safe_url());
    assert_eq!(first.public_headers(), second.public_headers());
    assert_eq!(first.timeout(), second.timeout());
    assert_eq!(first.debug(), second.debug());
}

api! {
    client ProofApi {
        base "https://example.test/api";
        scope resources(
            account: String,
            region: String,
        ) {
            path ["regions", scope.region, "scopes", scope.account, "resources"];
            GET ById(
                endpoint_value: String,
            ) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}

api! {
    client EqualNameApi {
        base "https://equal-name.example.test";
        scope resources(
            id: crate::MinimalValue,
        ) {
            path ["scopes", scope.id];
            GET ById(
                id: crate::MinimalValue,
            ) -> NoContent {
                path ["resources", ep.id];
            }
        }
    }
}

api! {
    client BoundsApi {
        base "https://bounds.example.test";
        scope resources(
            scope_value: crate::MinimalValue,
        ) {
            path ["scopes", scope.scope_value];
            GET ById(
                endpoint_value: crate::MinimalValue,
            ) -> NoContent {
                path ["resources", ep.endpoint_value];
            }
        }
    }
}

api! {
    client LazyApi {
        base "https://lazy.example.test";
        scope resources(scope_value: crate::CountingValue) {
            path ["scopes", scope.scope_value];
            GET ById(endpoint_value: crate::CountingValue) -> NoContent {
                path ["resources", ep.endpoint_value];
            }
        }
    }
}

mod nested_case {
    pub mod inner {
        use concord_macros::api;

        api! {
            client NestedApi {
                base "https://nested.example.test";
                scope resources(scope_value: ::std::string::String) {
                    path ["scopes", scope.scope_value, "resources"];
                    GET ById(endpoint_value: ::std::string::String) -> NoContent {
                        path [ep.endpoint_value];
                    }
                }
            }
        }
    }
}

mod repeated_case {
    #![allow(dead_code)]

    use concord_macros::api;

    struct Result;
    struct Option;
    struct String;
    struct Vec;
    struct Method;
    struct HeadBuilder;
    mod core {}
    mod runtime {}

    api! {
        client AlphaApi {
            base "https://alpha.example.test";
            scope resources(scope_value: ::std::string::String) {
                path ["scopes", scope.scope_value, "resources"];
                GET ById(endpoint_value: ::std::string::String) -> NoContent {
                    path [ep.endpoint_value];
                }
            }
        }
    }

    api! {
        client BetaApi {
            base "https://beta.example.test";
            scope resources(scope_value: ::std::string::String) {
                path ["scopes", scope.scope_value, "resources"];
                GET ById(endpoint_value: ::std::string::String) -> NoContent {
                    path [ep.endpoint_value];
                }
            }
        }
    }

    pub fn endpoint_names() -> (&'static str, &'static str) {
        let alpha = AlphaApi::new()
            .resources("alpha-scope".to_owned())
            .by_id("alpha-endpoint".to_owned());
        let beta = BetaApi::new()
            .resources("beta-scope".to_owned())
            .by_id("beta-endpoint".to_owned());
        let alpha = concord_core::__private::EndpointSpec::request_meta(&alpha);
        let beta = concord_core::__private::EndpointSpec::request_meta(&beta);
        (alpha.endpoint(), beta.endpoint())
    }
}

#[test]
fn facade_and_explicit_construction_have_exact_metadata_parity() {
    let facade_endpoint = {
        let facade =
            ProofApi::new().resources("account-alpha".to_owned(), "region-beta".to_owned());
        facade.by_id("item-omega".to_owned())
    };
    let explicit_endpoint = proof_api::endpoints::resources::ById::new(
        "account-alpha".to_owned(),
        "region-beta".to_owned(),
        "item-omega".to_owned(),
    );

    let facade_head = prepared_head(&facade_endpoint, "https://example.test/api");
    let explicit_head = prepared_head(&explicit_endpoint, "https://example.test/api");

    assert_same_head(&facade_head, &explicit_head);
    assert_eq!(facade_head.endpoint(), "ProofApi::resources::ById");
    assert_eq!(facade_head.method(), "GET");
    assert_eq!(
        facade_head.logical_url().as_str(),
        "https://example.test/api/regions/region-beta/scopes/account-alpha/resources/item-omega"
    );
    assert!(facade_head.public_headers().is_empty());
}

#[test]
fn explicit_constructor_preserves_distinct_scope_and_endpoint_values() {
    let endpoint = proof_api::endpoints::resources::ById::new(
        "outer-account-only".to_owned(),
        "outer-region-only".to_owned(),
        "endpoint-only".to_owned(),
    );
    let head = prepared_head(&endpoint, "https://example.test/api");

    assert_eq!(
        head.logical_url().as_str(),
        "https://example.test/api/regions/outer-region-only/scopes/outer-account-only/resources/endpoint-only"
    );
}

#[test]
fn equal_names_keep_scope_and_endpoint_bindings_distinct_without_extra_bounds() {
    let facade = EqualNameApi::new().resources(MinimalValue("outer-scope-id"));
    let cloned_facade = facade.clone();
    let facade_endpoint = cloned_facade.by_id(MinimalValue("endpoint-local-id"));
    let explicit_endpoint = equal_name_api::endpoints::resources::ById::new(
        MinimalValue("outer-scope-id"),
        MinimalValue("endpoint-local-id"),
    );

    let facade_head = prepared_head(&facade_endpoint, "https://equal-name.example.test");
    let explicit_head = prepared_head(&explicit_endpoint, "https://equal-name.example.test");
    assert_same_head(&facade_head, &explicit_head);
    assert_eq!(
        facade_head.logical_url().as_str(),
        "https://equal-name.example.test/scopes/outer-scope-id/resources/endpoint-local-id"
    );
}

#[test]
fn nested_module_invocation_preserves_generated_paths() {
    let facade = nested_case::inner::NestedApi::new().resources("nested-scope".to_owned());
    let endpoint = facade.by_id("nested-endpoint".to_owned());
    let explicit = nested_case::inner::nested_api::endpoints::resources::ById::new(
        "nested-scope".to_owned(),
        "nested-endpoint".to_owned(),
    );

    let endpoint = prepared_head(&endpoint, "https://nested.example.test");
    let explicit = prepared_head(&explicit, "https://nested.example.test");
    assert_same_head(&endpoint, &explicit);
}

#[test]
fn repeated_macro_invocations_keep_internal_aliases_isolated() {
    let (alpha, beta) = repeated_case::endpoint_names();
    assert_eq!(alpha, "AlphaApi::resources::ById");
    assert_eq!(beta, "BetaApi::resources::ById");
    assert_ne!(alpha, beta);
}

#[test]
fn generated_facade_requires_only_display_for_stored_values() {
    let facade = BoundsApi::new().resources(MinimalValue("scope-value"));
    let cloned_facade = facade.clone();
    let endpoint = cloned_facade.by_id(MinimalValue("endpoint-value"));
    let explicit = bounds_api::endpoints::resources::ById::new(
        MinimalValue("scope-value"),
        MinimalValue("endpoint-value"),
    );

    let endpoint = prepared_head(&endpoint, "https://bounds.example.test");
    let explicit = prepared_head(&explicit, "https://bounds.example.test");
    assert_same_head(&endpoint, &explicit);
    assert_eq!(
        endpoint.logical_url().as_str(),
        "https://bounds.example.test/scopes/scope-value/resources/endpoint-value"
    );
}

#[test]
fn generated_endpoint_construction_does_not_render_or_prepare_its_url() {
    let calls = Arc::new(AtomicUsize::new(0));
    let endpoint = LazyApi::new()
        .resources(CountingValue {
            text: "scope/value",
            calls: Arc::clone(&calls),
        })
        .by_id(CountingValue {
            text: "endpoint/value",
            calls: Arc::clone(&calls),
        });
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let head = prepared_head(&endpoint, "https://lazy.example.test");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        head.logical_url().as_str(),
        "https://lazy.example.test/scopes/scope%2Fvalue/resources/endpoint%2Fvalue"
    );
}
