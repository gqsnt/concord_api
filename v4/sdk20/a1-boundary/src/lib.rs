//! A1: the same five paths written against the two candidate response boundaries.
//!
//! - `owned`: `Prepared::receive() -> Received<T>` (implemented in core).
//! - `guard`: the smallest head-observer alternative, `send_observed(prepared, |head| ..)`,
//!   written here only over the public API (`into_parts` + `Decoder`), as an SDK or a core helper
//!   would provide it.
//!
//! Paths (modelled on the GitHub, Stripe, Shopify and Salesforce metadata sends and the
//! Kubernetes watch): metadata kept on decode failure; non-2xx provider document; a provider
//! refusal of a 2xx head that keeps the response unread; a usage header; an incremental reader
//! that must name the endpoint.
#![allow(missing_docs, dead_code)]

use concord::jsonl::JsonValuesReader;
use concord::reqwest::{self, header::HeaderMap};
use concord::{Error, Json, Prepared, ResponseHead};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: u64,
}

concord::api! {
    pub client Api {
        base "http://api.invalid";
        format Json;
        GET item(id: u64) -> Item { path "items/{id}"; }
        GET watch() { path "items"; query { "watch" = "1"; } }
        GET watch_stream() -> stream { path "items"; query { "watch" = "1"; } }
    }
}

#[derive(Debug, Clone)]
pub struct Meta {
    pub status: u16,
    pub request_id: Option<String>,
}

impl Meta {
    fn of(head: &ResponseHead<'_>) -> Self {
        Self {
            status: head.status().as_u16(),
            request_id: header(head.headers(), "x-request-id"),
        }
    }
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_owned)
}

pub enum SdkError {
    Concord(Error, Option<Meta>),
    WrongVersion(Option<String>, reqwest::Response),
}

/// The owned boundary: one public state, every path below uses the same three steps.
pub mod owned {
    use super::*;

    // 1. metadata + typed value; metadata kept on decode failure (GitHub/Salesforce).
    pub async fn with_meta<T>(p: Prepared<T>) -> Result<(T, Meta), SdkError> {
        let received = p.receive().await.map_err(|e| SdkError::Concord(e, None))?;
        let meta = Meta::of(&received.head());
        match received.decode().await {
            Ok(v) => Ok((v, meta)),
            Err(e) => Err(SdkError::Concord(e, Some(meta))),
        }
    }

    // 2. non-2xx provider document (Stripe): the unread response comes with the Status error.
    pub async fn document<T>(p: Prepared<T>) -> Result<T, (Meta, Option<String>)> {
        let received = p.receive().await.map_err(|_| (Meta { status: 0, request_id: None }, None))?;
        let meta = Meta::of(&received.head());
        match received.decode().await {
            Ok(v) => Ok(v),
            Err(e) => {
                let doc = match e.into_response() {
                    Ok(r) => r.text().await.ok(),
                    Err(_) => None,
                };
                Err((meta, doc))
            }
        }
    }

    // 3. provider refusal of a 2xx head, response kept unread (Shopify served version).
    pub async fn version_guarded<T>(p: Prepared<T>, want: &str) -> Result<T, SdkError> {
        let received = p.receive().await.map_err(|e| SdkError::Concord(e, None))?;
        if received.status().is_success() {
            let served = header(received.head().headers(), "x-api-version");
            if served.as_deref() != Some(want) {
                return Err(SdkError::WrongVersion(served, received.into_response()));
            }
        }
        received.decode().await.map_err(|e| SdkError::Concord(e, None))
    }

    // 5. incremental reader with automatic identity (Kubernetes watch): `-> stream`.
    pub async fn watch(api: &Api) -> Result<concord::jsonl::JsonValues<serde_json_value::Any>, String> {
        let incoming = api.watch_stream().send().await.map_err(|e| e.to_string())?;
        JsonValuesReader::new().open(incoming).map_err(|e| e.to_string())
    }
}

/// The head-guard alternative: a callback over the borrowed head, written over `into_parts`.
pub mod guard {
    use super::*;

    pub enum Observed<R> {
        Concord(Error),
        /// The guard refused: the response is handed back unread with its reason.
        Refused(R, reqwest::Response),
    }

    /// The smallest viable helper: execute once, let `observe` read or refuse the head, decode.
    pub async fn send_observed<T, M, R>(
        p: Prepared<T>,
        observe: impl FnOnce(&ResponseHead<'_>) -> Result<M, R>,
    ) -> (Option<M>, Result<T, Observed<R>>) {
        let (client, request, decoder) = p.into_parts();
        let response = match client.execute(request).await {
            Ok(r) => r,
            Err(e) => return (None, Err(Observed::Concord(decoder.transport_error(e)))),
        };
        match observe(&ResponseHead::of(&response)) {
            Ok(m) => (Some(m), decoder.decode(response).await.map_err(Observed::Concord)),
            Err(r) => (None, Err(Observed::Refused(r, response))),
        }
    }

    // 1. metadata + value: fits.
    pub async fn with_meta<T>(p: Prepared<T>) -> Result<(T, Meta), SdkError> {
        match send_observed(p, |h| Ok::<_, ()>(Meta::of(h))).await {
            (Some(m), Ok(v)) => Ok((v, m)),
            (m, Err(Observed::Concord(e))) => Err(SdkError::Concord(e, m)),
            // The refusal arm is impossible for an infallible observer, but the types require it.
            (_, Err(Observed::Refused((), _))) | (None, Ok(_)) => unreachable!(),
        }
    }

    // 3. provider refusal: fits, through the extra `Refused` arm.
    pub async fn version_guarded<T>(p: Prepared<T>, want: &str) -> Result<T, SdkError> {
        let (_, result) = send_observed(p, |h| {
            let served = header(h.headers(), "x-api-version");
            if !h.status().is_success() || served.as_deref() == Some(want) {
                Ok(())
            } else {
                Err(served)
            }
        })
        .await;
        result.map_err(|o| match o {
            Observed::Concord(e) => SdkError::Concord(e, None),
            Observed::Refused(served, response) => SdkError::WrongVersion(served, response),
        })
    }

    // 5. incremental reader: the helper decodes with the finite decoder, so a stream needs a
    //    second helper; the declared endpoint stays bodyless (its `send()` drains) and the reader
    //    must be named by hand because the guard never exposes the identity.
    pub async fn watch(api: &Api) -> Result<concord::jsonl::JsonValues<serde_json_value::Any>, String> {
        let response = api.watch().prepare().map_err(|e| e.to_string())?.send_raw().await.map_err(|e| e.to_string())?;
        JsonValuesReader::new()
            .open_for(response, "Api.watch") // duplicated name string
            .map_err(|e| e.to_string())
    }
}

/// A stand-in item type for the watch paths.
pub mod serde_json_value {
    #[derive(Debug, serde::Deserialize)]
    pub struct Any {}
}
