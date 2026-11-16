// Library builds use these placeholders; tests compile `wire.rs` with `quick-xml`.

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
