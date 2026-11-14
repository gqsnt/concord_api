use concord::page::{Call, Slot};
use concord::{reqwest, Answer, Body, Fail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Elasticsearch's error response: `{"error": {"type": …, "reason": …}, "status": 404}`.
#[derive(Debug, Deserialize)]
pub struct EsProblem {
    pub error: EsCause,
    pub status: u16,
}

#[derive(Debug, Deserialize)]
pub struct EsCause {
    #[serde(rename = "type")]
    pub kind: String,
    pub reason: Option<String>,
}

/// A point in time, as a search names it.
#[derive(Clone, Debug, Serialize)]
pub struct Pit {
    pub id: String,
    pub keep_alive: String,
}

/// `body ndjson(lines);`: one JSON value on each line, and a line break after the last one.
pub fn ndjson(lines: &[Value]) -> Result<Body, serde_json::Error> {
    let mut bytes = Vec::new();
    for line in lines {
        serde_json::to_writer(&mut bytes, line)?;
        bytes.push(b'\n');
    }
    Ok(Body::new(bytes, "application/x-ndjson"))
}

/// A `404` with `"found": false` is a missing document, a normal response; with `"error"`, it
/// is an error.
pub fn document_404<T: DeserializeOwned>(body: Value) -> Result<T, Fail<EsProblem>> {
    if body.get("error").is_some() {
        let problem: EsProblem = serde_json::from_value(body)
            .map_err(|_| concord::mismatch("not an Elasticsearch error"))?;
        return Err(problem.into());
    }
    serde_json::from_value(body).map_err(|_| concord::mismatch("not a missing document"))
}

/// The next page of a search in a point in time: the newest id of the point in time, and the
/// sort values of the last hit. An empty page is the end. A page that timed out, or to which some
/// shards did not respond, ends pagination with a Mismatch: going on would skip documents.
pub fn pit_after<C, P, A>(pit: P, after: A)
    -> impl Fn(C, &Answer<SearchPage>) -> Result<Option<C>, Fail<EsProblem>> + Send + 'static
where
    C: Call<Value = SearchPage, Problem = EsProblem>,
    P: Slot<C, Value = Pit>,
    A: Slot<C, Value = Vec<Value>>,
{
    move |call, answer| {
        let page = answer.value();
        if page.timed_out || page.shards.failed != 0 {
            return Err(concord::mismatch("a page with missing results"));
        }
        let Some(last) = page.hits.hits.last() else { return Ok(None) };
        let id = page.pit_id.clone().ok_or_else(|| concord::mismatch("no pit_id"))?;
        let keep_alive = pit.get(&call).map(|old| old.keep_alive.clone()).unwrap_or_default();
        let call = pit.set(call, Pit { id, keep_alive });
        Ok(Some(after.set(call, last.sort.clone())))
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CatFormat {
    Text,
    Json,
}

#[derive(Debug)]
pub enum CatReply {
    Text(String),
    Json(Vec<Value>),
}

/// The arm of `cat_indices`: the text as it arrived, or the rows when JSON was requested.
pub fn cat_reply(text: String, format: &Option<CatFormat>) -> Result<CatReply, Fail<EsProblem>> {
    match format {
        Some(CatFormat::Json) => serde_json::from_str(&text)
            .map(CatReply::Json)
            .map_err(|_| concord::mismatch("not JSON rows")),
        _ => Ok(CatReply::Text(text)),
    }
}

impl elasticsearch::indices::Search {
    /// The same search, sent as a `POST`: for a proxy that drops the body of a `GET`.
    pub fn post(self) -> Self {
        self.edit_request(|request| *request.method_mut() = reqwest::Method::POST)
    }
}
