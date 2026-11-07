//! The client's send policy: what every generated `send()` does. The policy is the
//! [`Connection`] itself (`send connection;`), so the realm of the path and the token applied to
//! the request come from one value.
//!
//! - **Reads** (`GET`): the connection's access token is applied; a 401 discards exactly that
//!   token, acquires one fresh token and sends the read once more. A second 401 is
//!   [`AuthorizationKind::AccessTokenRejected`](crate::AuthorizationKind::AccessTokenRejected).
//!   Nothing else is repeated.
//! - **Writes** (`POST`, invoice or payment create): sent exactly once. A 401 discards the token
//!   and is returned (the caller may send the same request ID again); no response, a 5xx or an
//!   unreadable 2xx is [`Error::Uncertain`] with the request ID; any other non-2xx is
//!   [`Error::Provider`].
//! - A 2xx carrying a `Fault` is [`Error::Provider`] with status 200, for reads and writes.
//!
//! A failed token acquisition is [`Error::Authorization`] and nothing is sent to the
//! Accounting API.

use concord::reqwest::{self, StatusCode, header::AUTHORIZATION};
use concord::{Decoder, Endpoint, Received, SendWith};
use concord_auth::Snapshot;

use crate::{
    AuthorizationError, Connection, Error, Fault, FaultInSuccess, ProviderError, RequestId,
    UncertainCause, UncertainWrite,
};

impl<T: Send + 'static> SendWith<T> for Connection {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let operation = E::operation();
        let (client, request, decoder) = endpoint.prepare()?.into_parts();
        if operation.method() == Some("GET") {
            self.read(&client, request, decoder).await
        } else {
            self.write(&client, request, decoder, operation.name())
                .await
        }
    }
}

impl Connection {
    async fn read<T>(
        &self,
        client: &reqwest::Client,
        request: reqwest::Request,
        decoder: Decoder<T>,
    ) -> Result<T, Error> {
        let again = request.try_clone();
        let used = self.access_token(client).await?;
        let received = execute(client, request, decoder, &used).await?;
        if received.status() != StatusCode::UNAUTHORIZED {
            return finish(received).await;
        }
        self.discard(&used);
        let Some(request) = again else {
            return Err(self.rejected(received).await);
        };
        drop(received);
        let used = self.access_token(client).await?;
        let received = execute(client, request, decoder, &used).await?;
        if received.status() == StatusCode::UNAUTHORIZED {
            self.discard(&used);
            return Err(self.rejected(received).await);
        }
        finish(received).await
    }

    async fn write<T>(
        &self,
        client: &reqwest::Client,
        request: reqwest::Request,
        decoder: Decoder<T>,
        operation: &'static str,
    ) -> Result<T, Error> {
        // The policy sees the built request only: the declared `requestid` is read back from it.
        let request_id = request
            .url()
            .query_pairs()
            .find(|(key, _)| key == "requestid")
            .and_then(|(_, value)| RequestId::new(value.into_owned()).ok());
        let uncertain = |cause| {
            Error::Uncertain(Box::new(UncertainWrite::new(
                operation,
                self.realm().clone(),
                request_id.clone(),
                cause,
            )))
        };
        let used = self.access_token(client).await?;
        let received = match execute(client, request, decoder, &used).await {
            Ok(received) => received,
            Err(e) => return Err(uncertain(UncertainCause::NoResponse(e))),
        };
        let status = received.status();
        if status == StatusCode::UNAUTHORIZED {
            self.discard(&used);
            return Err(self.rejected(received).await);
        }
        if status.is_server_error() {
            let provider = ProviderError::read(received).await;
            return Err(uncertain(UncertainCause::ServerError(Box::new(provider))));
        }
        if !status.is_success() {
            return Err(Error::Provider(Box::new(
                ProviderError::read(received).await,
            )));
        }
        match received.decode().await {
            Ok(value) => Ok(value),
            Err(e) => match fault_in(&e) {
                Some(provider) => Err(Error::Provider(Box::new(provider))),
                None => Err(uncertain(UncertainCause::UnreadableSuccess(e))),
            },
        }
    }

    async fn rejected<T>(&self, received: Received<T>) -> Error {
        let response = ProviderError::read(received).await;
        Error::Authorization(Box::new(AuthorizationError::access_rejected(
            self.realm().clone(),
            response,
        )))
    }
}

/// Applies the snapshot's credential immediately before the one execution.
async fn execute<T>(
    client: &reqwest::Client,
    mut request: reqwest::Request,
    decoder: Decoder<T>,
    used: &Snapshot,
) -> Result<Received<T>, concord::Error> {
    request
        .headers_mut()
        .insert(AUTHORIZATION, used.header().clone());
    decoder.execute(client, request).await
}

/// The declared handling of a read's final response.
async fn finish<T>(received: Received<T>) -> Result<T, Error> {
    if !received.status().is_success() {
        return Err(Error::Provider(Box::new(
            ProviderError::read(received).await,
        )));
    }
    received.decode().await.map_err(|e| match fault_in(&e) {
        Some(provider) => Error::Provider(Box::new(provider)),
        None => Error::Concord(e),
    })
}

/// The `Fault` that [`QboJson`](crate::QboJson) found in a 2xx body, with that response's head.
fn fault_in(error: &concord::Error) -> Option<ProviderError> {
    let fault: &Fault = &error
        .codec_error()?
        .source_unredacted()?
        .downcast_ref::<FaultInSuccess>()?
        .0;
    Some(ProviderError::in_success(error.head_meta()?, fault.clone()))
}
