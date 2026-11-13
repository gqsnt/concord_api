// Included in each API module, so all names resolve at its authored scope.
use bytes::Bytes;
use concord::page::Slot;
use concord::read::Stream;
use concord::{
    by_cursor, by_link, by_offset, by_offset_limit, by_page, form, http, json, Base, Binary, Body,
    ByteStream, Json, Secret, Text,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, json as j, Value};
use std::{collections::HashMap, time::Duration};

/// Inert data: the compile probe makes no provider-shape claim.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Stub {
    id: String,
}
impl Stub {
    /// Inert constructor.
    pub fn new() -> Self {
        Self::default()
    }
    /// Inert live record.
    pub fn live(_: Self) -> Option<Self> {
        Some(Self::default())
    }
    /// Inert continuation.
    pub fn next<T>(&self) -> Option<T> {
        None
    }
    /// Inert continuation.
    pub fn next_token(&self) -> Option<String> {
        None
    }
    /// Inert continuation.
    pub fn next_cursor(&self) -> Option<String> {
        None
    }
    /// Inert patch body.
    pub fn to_body(&self) -> Body {
        Body::new("", "application/json")
    }
    /// Inert patch classification.
    pub fn is_idempotent(&self) -> bool {
        false
    }
}
impl std::fmt::Display for Stub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stub")
    }
}
impl concord::Middleware for Stub {
    async fn handle(
        &self,
        request: concord::reqwest::Request,
        info: concord::transport::TryInfo,
        next: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        next.send(request, info).await
    }
}
/// Inert success envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StubReply<T> {
    result: T,
}
impl<T> StubReply<T> {
    /// Extract the typed value.
    pub fn into_result(self) -> Result<T, concord::Fail<ProbeProblem>> {
        Ok(self.result)
    }
}
/// An inert observation middleware.
#[derive(Clone, Debug)]
pub struct ObserveQuota<T>(T);
impl<T: Send + Sync + 'static> concord::Middleware for ObserveQuota<T> {
    async fn handle(
        &self,
        r: concord::reqwest::Request,
        i: concord::transport::TryInfo,
        n: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        n.send(r, i).await
    }
}
/// An inert signing middleware.
#[derive(Clone, Debug)]
pub struct Signed;
impl Signed {
    /// An inert signer.
    pub fn new<A, B>(_: A, _: B) -> Self {
        Self
    }
}
impl concord::Middleware for Signed {
    async fn handle(
        &self,
        r: concord::reqwest::Request,
        i: concord::transport::TryInfo,
        n: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        n.send(r, i).await
    }
}
/// An inert reader, using JSON only to discharge the reader contract.
#[derive(Clone, Copy, Debug)]
pub struct Xml;
impl<T: serde::de::DeserializeOwned + Send> concord::read::Reader<T> for Xml {
    async fn read(&self, incoming: concord::read::Incoming) -> Result<T, concord::read::ReadError> {
        concord::read::Reader::read(&Json, incoming).await
    }
}
/// Inert watch reader.
pub const WatchLines: Xml = Xml;
/// Inert body helper.
fn slack_json<T>(_: T) -> Body {
    Body::new("", "application/json")
}
fn slack_form<T>(_: T) -> Body {
    Body::new("", "application/x-www-form-urlencoded")
}
fn params<T>(_: T) -> Body {
    Body::new("", "application/json")
}
fn stripe_form<T, U>(_: T, _: U) -> Body {
    Body::new("", "application/x-www-form-urlencoded")
}
fn create_bucket_body<T>(_: T) -> Body {
    Body::new("", "application/xml")
}
fn copy_ok<T>(_: Value) -> Result<T, concord::Fail<ProbeProblem>> {
    Err(concord::mismatch("stub"))
}
fn complete_ok<T>(_: Value) -> Result<T, concord::Fail<ProbeProblem>> {
    Err(concord::mismatch("stub"))
}
fn document_404<T>(_: Value) -> Result<T, concord::Fail<ProbeProblem>> {
    Err(concord::mismatch("stub"))
}
fn next_cursor<T>(_: &T) -> Option<String> {
    None
}
fn telegram_result<T>(body: StubReply<T>) -> Result<T, concord::Fail<ProbeProblem>> {
    Ok(body.result)
}
fn slack_response<T>(body: StubReply<T>) -> Result<T, concord::Fail<ProbeProblem>> {
    Ok(body.result)
}
fn checked_repo_kind<T: Clone, U, V>(
    kind: &Option<T>,
    _: U,
    _: V,
) -> Result<Option<T>, std::io::Error> {
    Ok(kind.clone())
}
fn checked_cursor<
    C: concord::page::Call,
    S: concord::page::Slot<C>,
    F: Fn(&C::Value) -> Option<S::Value> + Send + 'static,
>(
    _: S,
    _: F,
) -> impl Fn(C, &concord::Answer<C::Value>) -> Option<C> + Send {
    |_, _| None
}
fn uploads_after<C: concord::page::Call, S, F>(
    _: S,
    _: F,
) -> impl Fn(C, &concord::Answer<C::Value>) -> Option<C> + Send {
    |_, _| None
}
fn by_object_id<C: concord::page::Call, S, F>(
    _: S,
    _: F,
) -> impl Fn(C, &concord::Answer<C::Value>) -> Option<C> + Send {
    |_, _| None
}
fn pit_after<C: concord::page::Call, S>(
    _: S,
) -> impl Fn(C, &concord::Answer<C::Value>) -> Option<C> + Send {
    |_, _| None
}
fn pit_body<A, B, C, D, E>(_: A, _: B, _: C, _: D, _: E) -> Value {
    Value::Null
}
fn ndjson<T>(_: T) -> Body {
    Body::new("", "application/x-ndjson")
}
fn cat_reply<T, F>(_: String, _: F) -> Result<T, concord::Fail<ProbeProblem>> {
    Err(concord::mismatch("stub"))
}
fn media_body<A, B, C, D, E, F>(_: A, _: B, _: C, _: D, _: E, _: F) -> Body {
    Body::new("", "application/octet-stream")
}
fn webhook_body<A, B, C, D>(_: A, _: B, _: C, _: D) -> Body {
    Body::new("", "application/json")
}
fn upload<A, B, C>(_: A, _: B, _: C) -> Body {
    Body::new("", "application/octet-stream")
}

const AttemptTrace: Stub = Stub { id: String::new() };
const TraceRequests: Stub = Stub { id: String::new() };
const KubeWarnings: Stub = Stub { id: String::new() };
const API_VERSION: &str = "stub";
fn new_idempotency_key() -> String {
    "stub".into()
}
fn AsJson<T>(_: &T) -> String {
    String::new()
}
fn comma<T>(_: &[T]) -> String {
    String::new()
}
fn copy_source<A, B, C>(_: A, _: B, _: C) -> String {
    String::new()
}

fn multipart<A, B, C>(_: A, _: B, _: C) -> Body {
    Body::new("", "multipart/form-data")
}

macro_rules! model {
    ($name:ident) => {
        #[doc = concat!("Compile-only nominal model `", stringify!($name), "`.")]
        #[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
        pub struct $name {
            id: String,
        }
        impl $name {
            /// Inert continuation.
            pub fn next<T>(&self) -> Option<T> {
                None
            }
            /// Inert continuation.
            pub fn next_token(&self) -> Option<String> {
                None
            }
            /// Inert continuation.
            pub fn next_cursor(&self) -> Option<String> {
                None
            }
            /// Inert patch body.
            pub fn to_body(&self) -> Body {
                Body::new("", "application/json")
            }
            /// Inert patch classification.
            pub fn is_idempotent(&self) -> bool {
                false
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("stub")
            }
        }
    };
    ($name:ident<T>) => {
        #[doc = concat!("Compile-only nominal sequence `", stringify!($name), "`.")]
        #[derive(Clone, Debug, Serialize, Deserialize)]
        pub struct $name<T> {
            data: Vec<T>,
        }
        impl<T> $name<T> {
            /// Inert continuation.
            pub fn next_token(&self) -> Option<String> {
                None
            }
            /// Inert continuation.
            pub fn next_cursor(&self) -> Option<String> {
                None
            }
        }
        impl<T: Send> concord::page::IntoItems for $name<T> {
            type Item = T;
            fn into_items(self) -> Vec<T> {
                self.data
            }
            fn count(&self) -> usize {
                self.data.len()
            }
        }
    };
}
/// The compile-only live/deleted envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StubMaybe<T> {
    value: Option<T>,
}
impl<T> StubMaybe<T> {
    /// Extract the nominal live record.
    pub fn live(self) -> Option<T> {
        self.value
    }
}
