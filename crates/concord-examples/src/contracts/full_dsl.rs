//! The complete DSL topology in one compile-checked, importable declaration.
//!
//! Required features: none. Mode: construction-only and credential-free planning.
//! Documentation owner: `docs/spec.md`.
//! Proving test: `tests/construction.rs`.

use std::sync::Arc;

use concord::rate_limit::NoopRateLimiter;
use concord::{Bytes, api};

/// Retained client default used to demonstrate clear/reset distinction.
pub const DEFAULT_CLIENT_LABEL: &str = "client-default";
/// Retained scope default used by unbound snapshots.
pub const DEFAULT_SCOPE_LABEL: &str = "scope-default";
/// Retained endpoint default used by named request wrappers.
pub const DEFAULT_VERBOSE: bool = false;

// ANCHOR: core_contract
api! {
    // ANCHOR: client_declaration
    client ReferenceApi {
        base "https://reference.example.test/root/";
        values {
            tenant: String;
            #[default]
            locale?: String;
            page_size: u32 = 25;
            client_label?: &'static str =
                crate::contracts::full_dsl::DEFAULT_CLIENT_LABEL;
        }
        credentials {
            secret api_key;
            credential application_key = api_key(secret.api_key);
        }
        subdomain replace ["api"];
        path ["v1"];
        query {
            "tenant" = client.tenant;
            "locale" = client.locale;
        }
        headers { "X-Client" = "canonical-reference"; }
        timeout 15s;
        rate_limits {
            policy regional_reads(region) { id "concord.full_dsl.1";
                key [origin, region];
                cost 1;
                allow 200 per 1m;
            }
        }
        rate_limit { id "concord.full_dsl.2"; key [origin]; cost 1;
            allow 100 per 1s;
            allow 1000 per 1m;
        }

    // ANCHOR_END: client_declaration

    // ANCHOR: status_endpoint
    GET status(
        verbose?: bool = crate::contracts::full_dsl::DEFAULT_VERBOSE,
        #[default]
        tags?: Vec<&'static str>,
    ) -> Bytes via Binary {
        path ["status"];
        query {
            "verbose" = endpoint.verbose;
            "tag" = each endpoint.tags;
        }
        auth off;
        timeout 30s;
    }
    // ANCHOR_END: status_endpoint

    // ANCHOR: regional_scope
    scope regions(
        region: String,
        language?: String,
        scope_label?: &'static str =
            crate::contracts::full_dsl::DEFAULT_SCOPE_LABEL,
    ) {
        subdomain replace [regions.region, "api"];
        path ["regions", regions.region];
        query { "language" = regions.language; }
        headers { "X-Region" = regions.region; }
        auth { header "X-Api-Key" = application_key; }
        timeout 5s;
        rate_limit regional_reads(region = regions.region);

        METHOD("PROPFIND") fetch(id: String, tags: Vec<String>) -> String via Text {
            path [concat ["resource-", endpoint.id]];
            query { "tag" = each endpoint.tags; }
            headers { "Accept-Language" = regions.language; }
        }
    }
    // ANCHOR_END: regional_scope
}}
// ANCHOR_END: core_contract

/// Constructs representative direct and explicit requests with `set_*` controls.
// ANCHOR: value_controls
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    // ANCHOR: configured_client
    let client = ReferenceApi::builder()
        .set_tenant("tenant-a".to_owned())
        .set_locale(Some("fr".to_owned()))
        .omit_locale()
        .reset_locale()
        .set_page_size(50)
        .reset_page_size()
        .with_client_label("client-override")
        .omit_client_label()
        .reset_client_label()
        .api_key(
            concord::SecretString::try_new("example-api-key").expect("bounded example API key"),
        )
        // This construction-only example selects an explicit test limiter so a
        // rate-declaring client can never build through an implicit bypass.
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()?;
    // ANCHOR_END: configured_client

    // ANCHOR: request_controls
    let _: reference_api::StatusRequest = client
        .status()
        .with_verbose(true)
        .omit_verbose()
        .reset_verbose()
        .with_tags(vec!["stable", "documented"])
        .omit_tags()
        .reset_tags();
    // ANCHOR_END: request_controls

    // ANCHOR: direct_and_explicit
    let _: reference_api::regions::FetchRequest = client
        .regions("eu-west".to_owned())
        .with_language("fr".to_owned())
        .with_scope_label("scope-override")
        .omit_scope_label()
        .reset_scope_label()
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let endpoint = ReferenceApi::unbound()
        .regions("eu-west".to_owned())
        .with_language("fr".to_owned())
        .with_scope_label("scope-override")
        .omit_scope_label()
        .reset_scope_label()
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let _: reference_api::regions::FetchRequest = client.request(endpoint);
    // ANCHOR_END: direct_and_explicit
    Ok(())
}
// ANCHOR_END: value_controls
