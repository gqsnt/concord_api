//! The complete DSL topology in one compile-checked, importable declaration.
//!
//! Required features: none. Mode: construction-only and safe planning.
//! Documentation owners: `docs/reference/dsl.md` and the Core Guide.
//! Proving test: `tests/construction.rs`.

use concord::api;

/// Retained client default used to demonstrate clear/reset distinction.
pub const DEFAULT_CLIENT_LABEL: Option<&'static str> = Some("client-default");
/// Retained scope default used by explicit snapshots.
pub const DEFAULT_SCOPE_LABEL: Option<&'static str> = Some("scope-default");
/// Retained endpoint default used by named request wrappers.
pub const DEFAULT_VERBOSE: Option<bool> = Some(false);

// ANCHOR: core_contract
api! {
    // ANCHOR: client_declaration
    client ReferenceApi {
        base "https://reference.example.test/root/";
        vars {
            tenant: String;
            locale: Option<String> = default;
            page_size: u32 = 25;
            client_label: Option<&'static str> =
                crate::contracts::full_dsl::DEFAULT_CLIENT_LABEL;
        }
        credentials {
            secret api_key: String;
            credential application_key = api_key(secret.api_key);
        }
        host ["api"];
        path ["v1"];
        query {
            "tenant" = vars.tenant;
            "locale" = vars.locale;
        }
        headers { "X-Client" = "canonical-reference"; }
        timeout 15s;
        rate_limit {
            100 / 1s;
            1000 / 1m;
        }
    }
    // ANCHOR_END: client_declaration

    // ANCHOR: status_endpoint
    GET status(
        verbose: Option<bool> = crate::contracts::full_dsl::DEFAULT_VERBOSE,
        tags: Option<Vec<&'static str>> = default,
    ) -> Bytes {
        path ["status"];
        query {
            "verbose" = ep.verbose;
            "tag" = ep.tags;
        }
        auth off;
        timeout off;
    }
    // ANCHOR_END: status_endpoint

    // ANCHOR: regional_scope
    scope regions(
        region: String,
        language: Option<String>,
        scope_label: Option<&'static str> =
            crate::contracts::full_dsl::DEFAULT_SCOPE_LABEL,
    ) {
        host [scope.region, "api"];
        path ["regions", scope.region];
        query { "language" = scope.language; }
        headers { "X-Region" = scope.region; }
        auth header "X-Api-Key" = application_key challenge never;
        timeout 5s;

        METHOD("PROPFIND") fetch(id: String, tags: Vec<String>) -> Text {
            path [fmt["resource-", ep.id]];
            query { "tag" = ep.tags; }
            headers { "Accept-Language" = scope.language; }
        }
    }
    // ANCHOR_END: regional_scope
}
// ANCHOR_END: core_contract

/// Constructs representative direct and explicit requests with `set_*` controls.
// ANCHOR: value_controls
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    // ANCHOR: configured_client
    let client = ReferenceApi::builder()
        .set_tenant("tenant-a".to_owned())
        .set_locale("fr".to_owned())
        .clear_locale()
        .reset_locale()
        .set_page_size(50)
        .reset_page_size()
        .set_client_label("client-override")
        .clear_client_label()
        .reset_client_label()
        .api_key(
            concord::SecretString::try_new("example-api-key").expect("bounded example API key"),
        )
        .build()?;
    // ANCHOR_END: configured_client

    // ANCHOR: request_controls
    let _: reference_api::StatusRequest = client
        .status()
        .set_verbose(true)
        .clear_verbose()
        .reset_verbose()
        .set_tags(vec!["stable", "documented"])
        .clear_tags()
        .reset_tags();
    // ANCHOR_END: request_controls

    // ANCHOR: direct_and_explicit
    let _: reference_api::regions::FetchRequest = client
        .regions("eu-west".to_owned())
        .set_language("fr".to_owned())
        .set_scope_label("scope-override")
        .clear_scope_label()
        .reset_scope_label()
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let endpoint = reference_api::regions::Fetch::new(
        "eu-west".to_owned(),
        "42".to_owned(),
        vec!["stable".to_owned()],
    );
    let _: reference_api::regions::FetchRequest = client.request(
        endpoint
            .set_language("fr".to_owned())
            .set_scope_label("scope-override")
            .clear_scope_label()
            .reset_scope_label(),
    );
    // ANCHOR_END: direct_and_explicit
    Ok(())
}
// ANCHOR_END: value_controls
