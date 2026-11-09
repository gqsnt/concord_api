use concord::{Endpoint, SendWith};
use serde::de::DeserializeOwned;

use crate::{Envelope, Error, QueryResponse};

/// One HTTP attempt followed by bounded JSON/envelope validation. No native request mutation.
#[derive(Default)]
pub struct QueryPolicy;

impl<T: DeserializeOwned + Send> SendWith<Envelope<T>, crate::error::ErrorDocument>
    for QueryPolicy
{
    type Output = Result<QueryResponse<T>, Error>;

    async fn send<E: Endpoint<Envelope<T>, crate::error::ErrorDocument> + Send>(
        &self,
        endpoint: E,
    ) -> Self::Output {
        let received = endpoint
            .prepare()?
            .error_body_limit(crate::ERROR_BODY_LIMIT)
            .receive()
            .await?;

        let (envelope, meta) = received.decode_with_meta().await?;
        if let Some(problem) = envelope.error {
            return Err(Error::Api {
                meta: Box::new(meta),
                problem,
            });
        }
        if envelope.warnings.is_some() {
            return Err(Error::Warnings(Box::new(meta)));
        }
        let Some(data) = envelope.query else {
            return Err(Error::InvalidResponse(Box::new(meta)));
        };
        Ok(QueryResponse {
            data,
            continuation: envelope.continuation,
            meta,
        })
    }
}
