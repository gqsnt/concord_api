mod contracts {
    #[cfg(feature = "json")]
    mod json;
    #[cfg(feature = "rate-limit")]
    mod local;
    #[cfg(feature = "oauth2-client")]
    mod oauth2;
    mod pagination;
    mod public;
    #[cfg(any(feature = "csv", feature = "ndjson"))]
    mod records;
}
