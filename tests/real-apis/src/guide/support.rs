//! Stand-in models for the descriptions of the guide pages.
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

pub use crate::credential_signing::Proof;
pub use concord::Base;

// `responses-and-errors.md`, the value beside the flag: the types and the arm, as the page prints them.
#[derive(Debug, Deserialize)]
pub struct ChatProblem {
    pub error: String,
}
#[derive(Debug, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
}
pub fn envelope<T: serde::de::DeserializeOwned>(
    body: Bytes,
) -> Result<T, concord::Fail<ChatProblem>> {
    #[derive(Deserialize)]
    struct Head {
        ok: bool,
    }
    let head: Head = Json::decode(&body).map_err(concord::Fail::from_read)?;
    if head.ok {
        Json::decode(&body).map_err(concord::Fail::from_read)
    } else {
        let problem: ChatProblem = Json::decode(&body).map_err(concord::Fail::from_read)?;
        Err(problem)?
    }
}
#[derive(Clone, Serialize)]
pub struct DeleteOptions {
    pub grace_seconds: u32,
}
pub fn optional_json<T: Serialize>(
    value: &Option<T>,
) -> Result<concord::Body, concord::body::WriteError> {
    match value {
        Some(value) => concord::json(value),
        None => Ok(concord::Body::from(Vec::new())),
    }
}

// `pages.md`: the pages of a shop, one shape of list for each way to page.
pub use concord::page::{by_offset_total, Pager, Paging, Slot};
pub use concord::{by_link, by_next_url, by_offset, by_page, Answer};
data!(Order, Product, Review, Item, Change);
#[derive(Debug, Deserialize)]
pub struct Batch<T> {
    pub data: Vec<T>,
    pub has_more: bool,
    pub last_id: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct Counted<T> {
    pub items: Vec<T>,
    pub total: u64,
}
#[derive(Debug, Deserialize)]
pub struct Numbered<T> {
    pub rows: Vec<T>,
    pub page: u32,
    pub pages: u32,
}
#[derive(Debug, Deserialize)]
pub struct Linked<T> {
    pub results: Vec<T>,
    pub next: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct Hits {
    pub hits: Vec<Doc>,
    pub timed_out: bool,
    pub next: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct Changes {
    pub changes: Vec<Change>,
    pub next_page_token: Option<String>,
    pub new_start_page_token: Option<String>,
}
impl<T: Send> concord::page::IntoItems for Batch<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.data
    }
    fn count(&self) -> usize {
        self.data.len()
    }
}
impl<T: Send> concord::page::IntoItems for Counted<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}
impl<T: Send> concord::page::IntoItems for Numbered<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.rows
    }
    fn count(&self) -> usize {
        self.rows.len()
    }
}

// The descriptions and the test use the types and functions printed by the guide.
#[path = "../../fixtures/recipes/failure.rs"]
mod failure;
pub use concord::read::{RecordStream, Sse};
pub use failure::{check_ticket, message_event, MessageEvent, UploadTicket};
