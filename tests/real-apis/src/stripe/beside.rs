use concord::page::{Call, Slot};
use concord::{Answer, Body, Fail, Headers};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The version these types were written for.
pub const API_VERSION: &str = "2026-10-28.endive";

/// `body stripe_form { … }`: Stripe's form, with nested keys: `metadata[order]=7781`,
/// `items[0][price]=price_1`, `expand[0]=customer`. A `None` is left out.
pub fn stripe_form<T: Serialize>(params: T) -> Result<Body, BoxError> {
    let mut pairs = Vec::new();
    if let Value::Object(fields) = serde_json::to_value(params)? {
        for (key, value) in fields {
            flatten(key, value, &mut pairs);
        }
    }
    Ok(concord::form(pairs)?)
}

fn flatten(key: String, value: Value, pairs: &mut Vec<(String, String)>) {
    match value {
        Value::Null => {}
        Value::Bool(b) => pairs.push((key, b.to_string())),
        Value::Number(n) => pairs.push((key, n.to_string())),
        Value::String(s) => pairs.push((key, s)),
        Value::Array(items) => {
            for (i, item) in items.into_iter().enumerate() {
                flatten(format!("{key}[{i}]"), item, pairs);
            }
        }
        Value::Object(fields) => {
            for (k, v) in fields {
                flatten(format!("{key}[{k}]"), v, pairs);
            }
        }
    }
}

/// A page of a list.
#[derive(Debug, Deserialize)]
pub struct List<T> {
    pub data: Vec<T>,
    pub has_more: bool,
}

/// A page of a search.
#[derive(Debug, Deserialize)]
pub struct SearchResult<T> {
    pub data: Vec<T>,
    pub has_more: bool,
    pub next_page: Option<String>,
}

impl<T> SearchResult<T> {
    /// For `by_cursor(page, …)`: the token of the next page, while Stripe says there is one.
    pub fn next_token(&self) -> Option<String> {
        self.next_page.clone().filter(|_| self.has_more)
    }
}

impl<T: Send> concord::page::IntoItems for List<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.data
    }
    fn count(&self) -> usize {
        self.data.len()
    }
}

impl<T: Send> concord::page::IntoItems for SearchResult<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.data
    }
    fn count(&self) -> usize {
        self.data.len()
    }
}

/// What the pager of the lists needs of an object: its id.
pub trait Object {
    fn id(&self) -> &str;
}

/// The next call, none at the end, or a Mismatch.
pub type NextPage<C> = Result<Option<C>, Fail<<C as Call>::Problem>>;

/// `paginate by_object_id(starting_after, ending_before);`. The next page starts after the last
/// object; when the caller went back with `ending_before`, it ends before the first one.
/// `has_more` with no object is not the end: the pages end with a Mismatch.
pub fn by_object_id<C, A, B, T>(after: A, before: B)
    -> impl Fn(C, &Answer<C::Value>) -> NextPage<C> + Send + 'static
where
    C: Call<Value = List<T>>,
    A: Slot<C, Value = String>,
    B: Slot<C, Value = String>,
    T: Object,
{
    move |call, answer| {
        let page = answer.value();
        if !page.has_more {
            return Ok(None);
        }
        let back = before.get(&call).is_some();
        let edge = if back { page.data.first() } else { page.data.last() };
        let Some(id) = edge.map(|object| object.id().to_owned()) else {
            return Err(concord::mismatch("has_more, and no object to go on from"));
        };
        Ok(Some(if back { before.set(call, id) } else { after.set(call, id) }))
    }
}

/// A field that `expand` turns from an id into the object: the type after the arrow does not
/// change with `.expand(…)`, so the field holds both shapes.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Expandable<T> {
    Id(String),
    Object(Box<T>),
}

/// A customer that was deleted is still found by its id: `{"id": …, "deleted": true}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MaybeDeleted<T> {
    Deleted { id: String, deleted: bool },
    Live(T),
}

impl<T> MaybeDeleted<T> {
    /// For the arm of `customers.retrieve`: its path gives the type of `body`.
    pub fn live(self) -> Option<T> {
        match self {
            MaybeDeleted::Live(object) => Some(object),
            MaybeDeleted::Deleted { .. } => None,
        }
    }
}

/// Stripe's policy: its `Stripe-Should-Retry` header decides, and a `409` (one key, two
/// requests at the same time) is tried again. Every `POST` here has a key, so each may go again.
pub fn retry() -> stripe::Retry {
    stripe::Retry::standard()
        .unless(|tried| should_retry(tried.headers()) == Some(false))
        .when(|tried| {
            let asked = should_retry(tried.headers()) == Some(true);
            let busy = tried.status().is_some_and(|status| status == 409);
            (asked || busy).then(|| Duration::from_millis(500))
        })
}

fn should_retry(headers: Option<&Headers>) -> Option<bool> {
    match headers?.get("stripe-should-retry")? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
