#[test]
fn extras_public_types_satisfy_only_semantic_core_contracts() {
    struct CursorPage {
        items: Vec<u8>,
        next: Option<String>,
    }

    impl IntoIterator for CursorPage {
        type Item = u8;
        type IntoIter = std::vec::IntoIter<u8>;

        fn into_iter(self) -> Self::IntoIter {
            self.items.into_iter()
        }
    }

    impl concord_extras::pagination::HasNextCursor<String> for CursorPage {
        fn next_cursor(&self) -> Option<&String> {
            self.next.as_ref()
        }
    }

    #[cfg(feature = "json")]
    fn response_codec<T: concord_core::contracts::ResponseRepresentation<String>>() {}
    #[cfg(feature = "json")]
    fn request_codec<T: concord_core::contracts::RequestRepresentation<String>>() {}
    fn controller<Page, T: concord_core::pagination::PaginationController<Page>>() {}
    #[cfg(any(feature = "csv", feature = "ndjson"))]
    fn records<T: concord_core::contracts::ResponseRepresentation<Vec<String>>>() {}
    #[cfg(feature = "oauth2-client")]
    fn provider<T: concord_core::contracts::CredentialProvider>() {}
    #[cfg(feature = "rate-limit")]
    fn limiter<T: concord_core::rate_limit::RateLimiter>() {}

    controller::<Vec<u8>, concord_extras::pagination::OffsetPagination>();
    controller::<Vec<u8>, concord_extras::pagination::PagePagination>();
    controller::<CursorPage, concord_extras::pagination::CursorPagination<String>>();

    #[cfg(feature = "json")]
    {
        response_codec::<concord_extras::codec::Json>();
        request_codec::<concord_extras::codec::Json>();
    }
    #[cfg(feature = "csv")]
    {
        type ConfCsv = concord_extras::records::Csv<
            concord_extras::records::HasHeader,
            concord_extras::records::SemicolonDelimiter,
        >;

        records::<concord_extras::records::Csv>();
        records::<ConfCsv>();
        type HeaderlessTsv = concord_extras::records::Csv<
            concord_extras::records::NoHeader,
            concord_extras::records::TabDelimiter,
        >;
        records::<HeaderlessTsv>();
    }
    #[cfg(feature = "ndjson")]
    records::<concord_extras::records::NdJson>();
    #[cfg(feature = "oauth2-client")]
    provider::<concord_extras::auth::OAuth2ClientCredentials>();
    #[cfg(feature = "rate-limit")]
    {
        limiter::<concord_extras::rate_limit::LocalRateLimiter>();
    }
}
