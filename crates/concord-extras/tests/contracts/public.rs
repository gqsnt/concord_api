#[test]
fn extras_public_types_satisfy_only_semantic_core_contracts() {
    struct CursorPage {
        next: Option<String>,
    }

    impl concord_extras::pagination::HasNextCursor<String> for CursorPage {
        fn next_cursor(&self) -> Option<&String> {
            self.next.as_ref()
        }
    }

    #[cfg(feature = "json")]
    fn response_codec<T: concord_core::codec::DecodeBody>() {}
    #[cfg(feature = "json")]
    fn request_codec<T: concord_core::codec::EncodeBody>() {}
    fn controller<Page, T: concord_core::pagination::PaginationController<Page>>() {}
    #[cfg(any(feature = "csv", feature = "ndjson"))]
    fn decoder<T: concord_core::records::RecordDecoder>() {}
    #[cfg(feature = "oauth2-client")]
    fn provider<T: concord_core::contracts::CredentialProvider>() {}
    #[cfg(feature = "rate-limit-governor")]
    fn limiter<T: concord_core::rate_limit::RateLimiter>() {}
    #[cfg(feature = "rate-limit-governor")]
    fn observer<T: concord_core::rate_limit::RateLimitObserver>() {}

    controller::<(), concord_extras::pagination::OffsetPagination>();
    controller::<(), concord_extras::pagination::PagePagination>();
    controller::<CursorPage, concord_extras::pagination::CursorPagination<String>>();

    #[cfg(feature = "json")]
    {
        response_codec::<concord_extras::codec::Json<String>>();
        request_codec::<concord_extras::codec::Json<String>>();
    }
    #[cfg(feature = "csv")]
    {
        type ConfCsv<T> = concord_extras::records::Csv<
            T,
            concord_extras::records::HasHeader,
            concord_extras::records::SemicolonDelimiter,
        >;

        decoder::<concord_extras::records::Csv<String>>();
        decoder::<ConfCsv<String>>();
        decoder::<
            concord_extras::records::Csv<
                (String, u32),
                concord_extras::records::NoHeader,
                concord_extras::records::TabDelimiter,
            >,
        >();
    }
    #[cfg(feature = "ndjson")]
    decoder::<concord_extras::records::NdJson<String>>();
    #[cfg(feature = "oauth2-client")]
    provider::<concord_extras::auth::OAuth2ClientCredentials>();
    #[cfg(feature = "rate-limit-governor")]
    {
        limiter::<concord_extras::rate_limit::GovernorRateLimiter>();
        observer::<concord_extras::rate_limit::DefaultRateLimitObserver>();
    }
}
