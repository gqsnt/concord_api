mod support;

mod generated {
    mod accepted_bytes;
    mod any_stream;
    pub(crate) mod authentication;
    #[cfg(feature = "records")]
    mod buffered_records;
    pub(crate) mod pagination;
    pub(crate) mod rate_limits;
    #[cfg(any(feature = "csv", feature = "ndjson"))]
    mod record_formats;
    mod representation_inheritance;
    mod shaping;
    #[cfg(feature = "sse")]
    mod sse;
    mod surface;
}
