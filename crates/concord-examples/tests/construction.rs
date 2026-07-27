#![forbid(unsafe_code)]

#[cfg(feature = "default-tls")]
use concord::SecretString;

#[test]
fn imports_and_constructs_every_enabled_canonical_declaration() {
    let _minimal_builder = concord_examples::getting_started::minimal::MinimalApi::builder();
    let _reference_builder = concord_examples::contracts::full_dsl::ReferenceApi::builder();
    let _minimal =
        concord_examples::getting_started::minimal::construct().expect("minimal construction");

    #[cfg(feature = "default-tls")]
    {
        use concord_examples::contracts::full_dsl::ReferenceApi;

        let _single_dispatch =
            concord_examples::runtime_policy::production_http::build_single_dispatch_client()
                .expect("protocol recovery configuration");
        concord_examples::contracts::full_dsl::construct().expect("full DSL construction");
        let reference = ReferenceApi::builder()
            .set_tenant("tenant-a".to_owned())
            .api_key(SecretString::try_new("construction-key").expect("bounded construction key"))
            .build()
            .expect("reference client");
        let _explicit = concord_examples::contracts::explicit_endpoints::construct(&reference);
    }

    concord_examples::request_response::specifications::construct()
        .expect("generic specification construction");
    let _response_client = concord_examples::request_response::acquisition::construct()
        .expect("response acquisition construction");
    let _observed =
        concord_examples::runtime_policy::hooks::construct().expect("observability construction");
    let _retrying =
        concord_examples::runtime_policy::retries::construct().expect("managed retry construction");
    #[cfg(feature = "default-tls")]
    let _api_key = concord_examples::recipes::api_key::build("recipe-api-key")
        .expect("API-key recipe construction");
    concord_examples::runtime_endpoints::generated_and_runtime::construct()
        .expect("runtime endpoint construction");

    #[cfg(all(feature = "json", feature = "default-tls"))]
    {
        concord_examples::contracts::regional_scopes::construct()
            .expect("regional scope construction");
    }

    #[cfg(feature = "json")]
    {
        let pagination = concord_examples::pagination::PaginationApi::builder()
            .build()
            .expect("pagination client");
        concord_examples::pagination::offset::construct(&pagination);
        concord_examples::pagination::page_number::construct(&pagination);
        concord_examples::pagination::cursor::construct(&pagination);
        concord_examples::pagination::custom::construct(&pagination);
        concord_examples::pagination::search::construct(&pagination);
    }

    #[cfg(all(feature = "json", feature = "multipart"))]
    concord_examples::request_response::bodies::construct().expect("body workflow construction");

    #[cfg(all(feature = "oauth2-client", feature = "default-tls"))]
    {
        concord_examples::authentication::usage::construct().expect("authentication construction");
        let _oauth = concord_examples::recipes::oauth_refresh::build(
            "https://localhost/oauth/token",
            "recipe-client",
            "recipe-secret",
        )
        .expect("OAuth recipe construction");
    }

    #[cfg(feature = "rate-limit")]
    {
        concord_examples::rate_limiting::lexical::construct().expect("rate-limit construction");
        concord_examples::rate_limiting::partitioning::construct()
            .expect("rate-limit partitioning construction");
    }

    #[cfg(all(feature = "records", feature = "csv", feature = "ndjson"))]
    concord_examples::request_response::records::construct().expect("record construction");

    #[cfg(all(feature = "json", feature = "rate-limit", feature = "default-tls"))]
    concord_examples::riot::usage::construct().expect("Riot construction");
}
