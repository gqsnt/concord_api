use http::Method;

/// Stable metadata for one Concord-visible request execution.
///
/// Reqwest-internal resends are deliberately not represented here. The
/// metadata contains no physical-attempt index, retry counter, URL, headers,
/// or mutable runtime state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestExecutionMeta {
    pub endpoint: &'static str,
    pub method: Method,
    pub idempotent: bool,
    pub page_index: u32,
}
