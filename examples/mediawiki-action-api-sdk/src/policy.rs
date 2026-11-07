use concord::{Endpoint, Json, SendWith};
use serde::de::DeserializeOwned;

use crate::{ERROR_BODY_LIMIT, Envelope, Error, HttpFailure, QueryResponse, error::ErrorDocument};

/// One HTTP attempt followed by bounded JSON/envelope validation. No native request mutation.
#[derive(Default)]
pub struct QueryPolicy;

impl<T: DeserializeOwned + Send> SendWith<Envelope<T>> for QueryPolicy {
    type Output = Result<QueryResponse<T>, Error>;

    async fn send<E: Endpoint<Envelope<T>> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            let (meta, document) = received
                .document::<Json, ErrorDocument>(ERROR_BODY_LIMIT)
                .await;
            return Err(Error::Http(Box::new(HttpFailure { meta, document })));
        }
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
