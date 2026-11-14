// Library builds use this placeholder; tests compile the actual quota middleware with Tokio.

/// Stand-in for the page's quota: it waits for nothing.
#[derive(Clone, Default)]
pub struct Quota;
impl Quota {
    pub fn new() -> Quota {
        Quota
    }
}
impl concord::Middleware for Quota {
    async fn handle(
        &self,
        request: concord::reqwest::Request,
        info: concord::transport::TryInfo,
        next: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        next.send(request, info).await
    }
}
