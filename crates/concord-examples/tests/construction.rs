#![forbid(unsafe_code)]

#[cfg(feature = "default-tls")]
use concord::SecretString;

#[test]
fn imports_and_constructs_every_enabled_canonical_declaration() {
    let _minimal_builder = concord_examples::getting_started::minimal::MinimalApi::builder();
    let _reference_builder = concord_examples::contracts::full_dsl::ReferenceApi::builder();

    #[cfg(feature = "default-tls")]
    {
        use concord_examples::contracts::full_dsl::ReferenceApi;

        let _minimal =
            concord_examples::getting_started::minimal::construct().expect("minimal construction");
        concord_examples::contracts::full_dsl::construct().expect("full DSL construction");
        let reference = ReferenceApi::builder()
            .tenant("tenant-a".to_owned())
            .api_key(SecretString::new("construction-key"))
            .build()
            .expect("reference client");
        let _explicit = concord_examples::contracts::explicit_endpoints::construct(&reference);
    }

    concord_examples::request_response::specifications::construct()
        .expect("generic specification construction");
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
    }

    #[cfg(all(feature = "json", feature = "multipart"))]
    concord_examples::request_response::bodies::construct().expect("body workflow construction");

    #[cfg(all(feature = "oauth2-client", feature = "default-tls"))]
    concord_examples::authentication::usage::construct().expect("authentication construction");

    #[cfg(feature = "rate-limit-governor")]
    concord_examples::rate_limiting::lexical::construct().expect("rate-limit construction");

    #[cfg(all(feature = "records", feature = "csv", feature = "ndjson"))]
    concord_examples::request_response::records::construct().expect("record construction");

    #[cfg(all(feature = "dangerous-custom-retry", feature = "default-tls"))]
    concord_examples::dangerous::custom_retry::construct().expect("dangerous retry construction");

    #[cfg(all(feature = "dangerous-raw-response", feature = "json"))]
    concord_examples::dangerous::raw_response::construct()
        .expect("dangerous raw-response construction");

    #[cfg(all(
        feature = "json",
        feature = "rate-limit-governor",
        feature = "default-tls"
    ))]
    concord_examples::riot::usage::construct().expect("Riot construction");
}
