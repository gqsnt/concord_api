//! Managed runtime lifecycle, truthful execution journal and planning through
//! downstream-generated v3 clients.

mod runtime {
    pub(crate) mod deterministic;
    pub(crate) mod journal;
    #[cfg(feature = "multipart")]
    pub(crate) mod lifecycle;
    pub(crate) mod planning;
    // Runtime-endpoint suites cover prepared multipart bodies, the local
    // limiter and TLS-bound private auth.
    #[cfg(all(feature = "multipart", feature = "rate-limit", feature = "default-tls"))]
    mod runtime_endpoint;
}
