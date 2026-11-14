//! Stand-in models for independent regression descriptions.
#![allow(missing_docs, dead_code, unused_imports)]

pub use concord::bytes::Bytes;
pub use concord::read::Stream;
pub use concord::{by_cursor, by_offset_limit, form, json, Binary, Json, Secret, Text};
use serde::{Deserialize, Serialize};
pub use std::time::Duration;
macro_rules! data { ($($n:ident),*) => { $( #[derive(Clone, Debug, Serialize, Deserialize)] pub struct $n(pub String);
        impl std::fmt::Display for $n { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {self.0.fmt(f)} }
    )* } }
data!(
    Task,
    NewTask,
    RiotProblem,
    MatchRoute,
    Puuid,
    MatchId,
    Match,
    NewDoc,
    Doc,
    DocId,
    NewCharge,
    Charge,
    Declined,
    InvoiceId,
    Account,
    ChatId,
    Message,
    NewCustomer,
    Customer,
    StripeProblem,
    Balance,
    Update,
    FileId,
    Page,
    Asset,
    AccountRoute,
    Invoice,
    ParseMode,
    Region,
    Token
);
#[derive(Clone, Debug, Deserialize)]
pub struct List<T> {
    pub data: Vec<T>,
    pub next: Option<String>,
}
impl<T> List<T> {
    pub fn next_cursor(&self) -> Option<String> {
        self.next.clone()
    }
}
pub struct Created {
    pub doc: Doc,
    pub at: String,
}
pub struct Versioned<T> {
    pub etag: Option<String>,
    pub value: T,
}
pub fn created(_body: NewTask, h: &concord::Headers) -> Result<Created, concord::Fail<()>> {
    Ok(Created {
        doc: Doc("stub".into()),
        at: h.require("location")?.to_owned(),
    })
}
pub fn markup_of(_t: &String) -> Option<String> {
    None
}
#[derive(Clone)]
pub struct Signed;
impl Signed {
    pub fn new(_k: Secret) -> Self {
        Signed
    }
}
impl concord::Middleware for Signed {
    async fn handle(
        &self,
        request: concord::reqwest::Request,
        info: concord::transport::TryInfo,
        next: concord::middleware::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        next.send(request, info).await
    }
}
pub type Platform = String;
#[derive(Clone, Serialize)]
pub struct ChatRequest {
    pub stream: bool,
}
pub struct ChatReply;
pub struct ChatRead(pub bool);
impl concord::read::Reader<ChatReply> for ChatRead {
    async fn read(
        &self,
        _: concord::read::Incoming,
    ) -> Result<ChatReply, concord::read::ReadError> {
        Ok(ChatReply)
    }
}
pub use concord::multipart;
