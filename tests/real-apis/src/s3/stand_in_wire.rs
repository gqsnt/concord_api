// Library builds use these placeholders; tests compile `wire.rs` with `quick-xml` and `aws-sigv4`.

/// Stand-in for the page's XML reader: it reads JSON.
#[derive(Clone, Copy, Debug)]
pub struct Xml;
impl<T: serde::de::DeserializeOwned + Send> concord::Reader<T> for Xml {
    async fn read(&self, body: concord::read::Incoming) -> Result<T, concord::read::ReadError> {
        concord::Reader::read(&concord::Json, body).await
    }
}
/// Stand-in for the page's XML writer: it writes JSON.
pub fn xml<T: serde::Serialize>(value: &T) -> Result<concord::Body, concord::body::WriteError> {
    concord::json(value)
}
pub fn create_bucket_body(
    value: &Option<CreateBucketConfiguration>,
) -> Result<concord::Body, concord::body::WriteError> {
    concord::json(value)
}
/// Stand-in for the page's signer: it signs nothing.
pub struct Signed;
impl Signed {
    pub fn new(_: Credentials, _: String) -> Self {
        Signed
    }
}
impl concord::Middleware for Signed {
    async fn handle(
        &self,
        request: concord::reqwest::Request,
        info: concord::transport::TryInfo,
        next: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        next.send(request, info).await
    }
}
