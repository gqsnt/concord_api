mod contracts {
    #[cfg(feature = "rate-limit-governor")]
    mod governor;
    #[cfg(feature = "json")]
    mod json;
    #[cfg(feature = "oauth2-client")]
    mod oauth2;
    mod pagination;
    mod public;
    #[cfg(any(feature = "csv", feature = "ndjson"))]
    mod records;
}
