//! The complete DSL topology in one compile-checked, importable declaration.
//!
//! Required features: none. Mode: construction-only and safe planning.
//! Documentation owner: `docs/reference/dsl.md`.
//! Proving test: `tests/construction.rs`.

use concord::api;

api! {
    client ReferenceApi {
        base "https://reference.example.test/root/";
        vars {
            tenant: String;
            locale: Option<String>;
            page_size: u32 = 25;
        }
        credentials {
            secret api_key: String;
            credential application_key = api_key(secret.api_key);
        }
        retry off;
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

    GET status(verbose: Option<bool>) -> Bytes {
        path ["status"];
        query { "verbose" = ep.verbose; }
        auth off;
        timeout off;
    }

    scope regions(region: String, language: Option<String>) {
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
}

api! {
    client ReqwestRetryReference {
        base "https://retry-reference.example.test/";
        retry reqwest_default;
    }

    GET health() -> NoContent;
}

/// Constructs representative direct and explicit endpoint requests.
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let client = ReferenceApi::builder()
        .tenant("tenant-a".to_owned())
        .api_key(
            concord::SecretString::try_new("example-api-key").expect("bounded example API key"),
        )
        .build()?;
    let _: reference_api::StatusRequest = client.status().verbose(true);
    let _: reference_api::regions::FetchRequest = client
        .regions("eu-west".to_owned())
        .language("fr".to_owned())
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let endpoint = reference_api::regions::Fetch::new(
        "eu-west".to_owned(),
        "42".to_owned(),
        vec!["stable".to_owned()],
    );
    let _: reference_api::regions::FetchRequest =
        client.request(endpoint.language("fr".to_owned()));
    let retry_client = ReqwestRetryReference::builder().build()?;
    let _: reqwest_retry_reference::HealthRequest = retry_client.health();
    Ok(())
}
