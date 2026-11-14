//! Application data shared by the clients.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A task returned by the task service.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    /// The task's identifier.
    pub id: u64,
    /// The task's title.
    pub title: String,
    /// Whether the task has been completed.
    pub done: bool,
}

/// The fields needed to create a task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewTask {
    /// The title of the new task.
    pub title: String,
}

/// A regional routing group for Riot API requests.
///
/// The region is serialized as a lowercase host label, for example `europe`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchRoute {
    /// The Americas routing group.
    Americas,
    /// The Asia routing group.
    Asia,
    /// The Europe routing group.
    Europe,
    /// The Southeast Asia routing group.
    Sea,
}

/// A routing group for account requests, using the same labels as match requests.
pub type AccountRoute = MatchRoute;

/// Regional route values accepted by both Riot scope methods.
#[doc(inline)]
pub use MatchRoute::{Americas, Asia, Europe, Sea};

/// A player's persistent Riot identifier.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Puuid(
    /// The identifier exactly as supplied by Riot.
    pub String,
);

impl From<&str> for Puuid {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Display for Puuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A match identifier, including the provider's regional prefix.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MatchId(
    /// The complete match identifier, for example `EUW1_1`.
    pub String,
);

impl From<&str> for MatchId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Display for MatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The match details used by these examples.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Match {
    /// The match's identifier.
    pub id: MatchId,
}

/// A Riot player account. This is data; [`riot::Account`](crate::riot::Account) is a scope handle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    /// The player's persistent identifier.
    pub puuid: Puuid,
    /// The name before the `#` in the Riot ID.
    #[serde(rename = "gameName")]
    pub game_name: String,
    /// The tag after the `#` in the Riot ID.
    #[serde(rename = "tagLine")]
    pub tag_line: String,
}

/// The structured problem body supplied when Riot refuses a request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiotProblem {
    /// The provider's explanation and status code.
    pub status: RiotStatus,
}

/// The details inside a Riot problem body.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiotStatus {
    /// The provider's explanation of the refusal.
    pub message: String,
    /// The provider's status code from the problem body.
    pub status_code: u16,
}

/// A document identifier accepted by [`Files::exists`](crate::Files::exists).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocId(
    /// The document identifier supplied by the file service.
    pub String,
);

impl From<&str> for DocId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// A file identifier accepted by [`Files::download`](crate::Files::download).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileId(
    /// The file identifier supplied by the file service.
    pub String,
);

impl From<&str> for FileId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// One entry in a file search or feed page.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileEntry {
    /// The file's identifier.
    pub id: FileId,
    /// The file's name.
    pub name: String,
}

/// A page of file entries with an optional continuation cursor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    /// The entries in this page, in provider order.
    pub data: Vec<FileEntry>,
    /// The cursor for the next page, or `None` at the end.
    pub next_cursor: Option<String>,
}

impl Page {
    /// Borrow the continuation cursor, or return `None` after the final page.
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

impl IntoIterator for Page {
    type Item = FileEntry;
    type IntoIter = std::vec::IntoIter<FileEntry>;
    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl concord::page::IntoItems for Page {
    type Item = FileEntry;
    fn into_items(self) -> Vec<FileEntry> {
        self.data
    }
    fn count(&self) -> usize {
        self.data.len()
    }
}

/// A middleware that signs each request before sending through the rest of the chain.
///
/// This example uses a keyed checksum of the HTTP method and path. Replace the
/// checksum with the signature algorithm of the service when using a real API.
pub struct Signed {
    key: concord::Secret,
}

impl Signed {
    /// Sign each request with this key.
    pub fn new(key: impl Into<concord::Secret>) -> Self {
        Self { key: key.into() }
    }
}

impl fmt::Debug for Signed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signed").finish_non_exhaustive()
    }
}

impl concord::Middleware for Signed {
    async fn handle(
        &self,
        mut request: concord::reqwest::Request,
        info: concord::transport::TryInfo,
        next: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        let text = format!("{} {}", request.method(), request.url().path());
        let checksum = self
            .key
            .expose()
            .bytes()
            .chain([0])
            .chain(text.bytes())
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
        let mut signature = concord::http::HeaderValue::from_str(&format!("{checksum:016x}"))
            .map_err(|error| concord::transport::TransportError::blocked().with_source(error))?;
        signature.set_sensitive(true);
        request.headers_mut().insert("x-signature", signature);
        next.send(request, info).await
    }
}

/// The owned continuation cursor used by the feed's `by_cursor` pager.
pub fn page_cursor(page: &Page) -> Option<String> {
    page.next_cursor.clone()
}

/// An item of the shapes service.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// The item's identifier.
    pub id: u64,
    /// The item's title.
    pub title: String,
}

/// The structured problem body supplied when the shapes service refuses a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapesProblem {
    /// The provider's code for the refusal.
    pub code: String,
    /// The provider's explanation of the refusal.
    pub message: String,
}

/// What the shapes service says when an item is held by another change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// The provider's explanation of the conflict.
    pub message: String,
}

/// Where a created item was put.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// The status of the answer that created the item.
    pub status: u16,
    /// The address of the new item, when the service gave one.
    pub location: Option<String>,
}

/// A filter of items: a struct, which a query cannot hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    /// Keep the items of this owner.
    pub owner: String,
    /// Keep the open items only.
    pub open: bool,
}

/// One count of a [`Tally`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count {
    /// What was counted.
    pub kind: String,
    /// How many there are.
    pub n: u32,
}

/// Counts by name: an answer type with a map in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    /// The counts of each name.
    pub counts: std::collections::BTreeMap<String, Vec<Count>>,
}

/// What the shapes service sends around each answer under `wrapped`: a value or a problem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Envelope<T> {
    /// The value.
    Data(T),
    /// The service's problem, sent with a success status.
    Problem(ShapesProblem),
}

/// The value of an envelope, or the problem that it holds.
///
/// # Errors
///
/// The problem of an envelope that holds one.
pub fn unpack<T>(envelope: Envelope<T>) -> Result<T, ShapesProblem> {
    match envelope {
        Envelope::Data(value) => Ok(value),
        Envelope::Problem(problem) => Err(problem),
    }
}

/// The value of an answer under `sealed`, when its head carries the header `x-seal`.
///
/// It takes any value: a body that a reader read, or a stream that was handed over unread.
///
/// # Errors
///
/// A problem with the code `unsealed` when the head has no `x-seal`.
pub fn unseal<T>(body: T, headers: &concord::Headers) -> Result<T, ShapesProblem> {
    match headers.get("x-seal") {
        Some(_) => Ok(body),
        None => Err(ShapesProblem {
            code: "unsealed".to_owned(),
            message: "the answer carries no seal".to_owned(),
        }),
    }
}

/// An item with an empty title, which [`checked`] does not write.
#[derive(Debug)]
pub struct EmptyTitle;

impl fmt::Display for EmptyTitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an item needs a title")
    }
}

impl std::error::Error for EmptyTitle {}

/// The item as a JSON body, or an error when its title is empty.
///
/// # Errors
///
/// [`EmptyTitle`] for an empty title, and the error of [`concord::json`].
pub fn checked(item: &Item) -> Result<concord::Body, Box<dyn std::error::Error + Send + Sync>> {
    if item.title.is_empty() {
        return Err(Box::new(EmptyTitle));
    }
    Ok(concord::json(item)?)
}

/// Notes when each expression of a send runs, and gives what the test chose.
///
/// Clones share one list of notes.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    seen: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    repeat: bool,
    quiet: std::time::Duration,
}

impl Trace {
    /// A trace whose `idempotent` gives `repeat`, and whose `idle_timeout` gives `quiet`.
    pub fn new(repeat: bool, quiet: std::time::Duration) -> Trace {
        Trace {
            seen: Default::default(),
            repeat,
            quiet,
        }
    }

    /// For `idempotent`: note `"idempotent"`, and give `repeat`.
    pub fn idempotent(&self) -> bool {
        self.push("idempotent");
        self.repeat
    }

    /// For `idle_timeout`: note `"idle_timeout"`, and give `quiet`.
    pub fn idle_timeout(&self) -> std::time::Duration {
        self.push("idle_timeout");
        self.quiet
    }

    /// For an entry: note `step`, and give it as text.
    pub fn note(&self, step: &'static str) -> String {
        self.push(step);
        step.to_owned()
    }

    /// What was noted, in order.
    pub fn seen(&self) -> Vec<&'static str> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn push(&self, step: &'static str) {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(step);
    }
}

/// A reader of the author's own that never gives a value, and says why at length:
/// its message is longer than 1 KiB.
#[derive(Clone, Copy, Debug)]
pub struct Chatty;

impl Chatty {
    /// The message of every failure: 41 sentences, 2,050 bytes.
    pub fn message() -> String {
        "this reader reads nothing, and says so at length. ".repeat(41)
    }
}

impl<T: Send> concord::Reader<T> for Chatty {
    async fn read(&self, body: concord::read::Incoming) -> Result<T, concord::read::ReadError> {
        let bytes = body.bytes(64 * concord::KIB).await?;
        Err(concord::read::ReadError::not_the_type(bytes).with_details(Self::message()))
    }
}

/// One page of items of the shapes service, with what it says about the next one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    /// The items of this page, in provider order.
    pub items: Vec<Item>,
    /// The cursor or the address of the next page, or `None` at the end.
    #[serde(default)]
    pub next: Option<String>,
    /// Whether the service says that more items follow.
    #[serde(default)]
    pub more: bool,
    /// A refusal that the service sends inside a page.
    #[serde(default)]
    pub refusal: Option<ShapesProblem>,
}

impl Batch {
    /// The address of the next page, for `paginate by_next_url(Batch::next_url);`.
    pub fn next_url(&self) -> Option<String> {
        self.next.clone()
    }
}

impl concord::page::IntoItems for Batch {
    type Item = Item;
    fn into_items(self) -> Vec<Item> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// `paginate next_batch(cursor);`: the next page has the cursor that the page gives.
///
/// A page that holds a refusal ends the pages with that problem; a page that says that
/// more items follow and gives no cursor ends them with a Mismatch: it is not the end.
// The pager names its call, answer and problem types, as a shipped one does.
#[allow(clippy::type_complexity)]
pub fn next_batch<C, S>(
    cursor: S,
) -> impl Fn(C, &concord::Answer<Batch>) -> Result<Option<C>, concord::Fail<ShapesProblem>>
       + Send
       + 'static
where
    C: concord::page::Call<Value = Batch, Problem = ShapesProblem>,
    S: concord::page::Slot<C, Value = String>,
{
    move |call, answer| {
        let batch = answer.value();
        if let Some(problem) = &batch.refusal {
            return Err(problem.clone().into());
        }
        match (&batch.next, batch.more) {
            (Some(next), _) => Ok(Some(cursor.set(call, next.clone()))),
            (None, true) => Err(concord::mismatch("more items, and no cursor")),
            (None, false) => Ok(None),
        }
    }
}

/// A body that is a new stream each time it is made: `body stream();`.
///
/// # Errors
///
/// None in practice: the bytes are in memory.
pub fn stream() -> Result<concord::Body, std::io::Error> {
    let bytes = concord::Body::new("fresh", "text/plain").into_stream()?;
    Ok(concord::Body::stream(bytes).content_type("text/plain"))
}
