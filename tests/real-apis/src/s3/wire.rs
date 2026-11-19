use concord::read::{Incoming, ReadError, Reader};
use concord::Body;
use serde::{de::DeserializeOwned, Serialize};

/// `read Xml;`: the body, at most 10 MiB, read by `quick-xml`.
#[derive(Clone, Copy, Debug)]
pub struct Xml;

impl<T: DeserializeOwned + Send> Reader<T> for Xml {
    async fn read(&self, body: Incoming) -> Result<T, ReadError> {
        let bytes = body.bytes(10 * concord::MIB).await?;
        quick_xml::de::from_reader(&bytes[..])
            .map_err(|error| ReadError::not_the_type(bytes).with_details(error.to_string()))
    }
}

/// `body xml(batch);`
pub fn xml<T: Serialize>(value: &T) -> Result<Body, quick_xml::SeError> {
    Ok(Body::new(quick_xml::se::to_string(value)?, "application/xml"))
}
