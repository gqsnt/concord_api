# Elasticsearch

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [Telegram](telegram.md) · next: [all the examples](README.md)*

For a Rust developer who calls Elasticsearch: a client of 15 endpoints to copy from. It is the example of a `GET` with a body, of a body of many JSON lines, of a search walked page by page in a point in time, and of an answer whose format is an option.

Elasticsearch, as Concord sees it. The server is the deployment's own: `base url;`, an input of type `Base`. Who you are: a whole `authorization` header, a `Secret` (`concord::basic(user, password)`, or `ApiKey …`). The first piece of the path is an index, or several (`logs-*,metrics-*`) kept in one segment, so the tree has two handles: `index(name)` for one index, `indices(targets)` for a search over many. A search is a `GET` with a JSON body. A bulk write is many JSON lines, and its `200` holds a result for each line, failures too. A search over many pages walks a point in time, with the sort values of the last hit. Elasticsearch says no with `{"error": {"type", "reason"}, "status"}`, read into `EsProblem`; a missing document is a `404` that is an answer (`"found": false`). Elastic's own reference: [elastic.co/docs/api/doc/elasticsearch](https://www.elastic.co/docs/api/doc/elasticsearch). `(unsure)` marks a detail of Elastic's that is not checked.

## The description

```rust
use concord::{json, Base, Json, Secret, Text};
use serde_json::Value;

concord::api! {
    /// Elasticsearch: documents, searches, a point in time, bulk writes, and the `_cat` tables.
    pub client Elasticsearch(url: Base, authorization: Secret) {
        base url;
        headers { "authorization" = authorization; }
        read Json;
        problem EsProblem;

        part output(pretty?: bool, filter_path?: Vec<String>) {
            query { pretty; "filter_path" = filter_path.as_ref().map(|p| p.join(",")); }
        }
        part write_options(refresh?: Refresh, routing?: String) {
            query { refresh; routing; }
        }

        /// One index, or an alias of one index: not a wildcard, not a list.
        scope index(name: String) {
            path "{name}";

            /// Writing a document again makes a new version of it: not sent twice.
            PUT put(id: String, document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                body json(document);
                idempotent false;
            }
            POST add(document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc";
                body json(document);
            }
            /// A missing document is an answer (`found: false`); a missing index is an error.
            GET get(id: String, ..output) -> GetReply {
                path "_doc/{id}";
                status { 404 { body } => document_404(body)? }
            }
            HEAD exists(id: String) -> bool {
                path "_doc/{id}";
                status { 2xx => true, 404 => false }
            }
            /// `changes` is Elasticsearch's own envelope: `doc`, `script`, `upsert`.
            POST update(id: String, changes: Value, ..output, ..write_options) -> WriteReply {
                path "_update/{id}";
                body json(changes);
            }
            DELETE delete_document(id: String, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                status { 404 { body } => document_404(body)? }
                idempotent false;
            }
            PUT create(settings: Value, ..output) -> IndexAck {
                body json(settings);
                idempotent false;
            }
            DELETE delete(..output) -> IndexAck { idempotent false; }
        }

        /// Several indices: a list and wildcards in one segment, `logs-*,metrics-*`.
        scope indices(targets: String) {
            path "{targets}";

            /// A `GET` with a body. `.post()` sends the same search as a `POST`.
            GET search(request: Value, ..output) -> SearchPage {
                path "_search";
                body json(request);
            }
            GET count(request: Value, ..output) -> CountReply {
                path "_count";
                body json(request);
            }
            POST open_pit(keep_alive: String, ..output) -> OpenedPit {
                path "_pit";
                query { keep_alive; }
            }
        }

        /// Many writes in one request: a line of action, then a line of document, for each one.
        POST bulk(lines: Vec<Value>, ..output, ..write_options) -> BulkReply {
            path "_bulk";
            body ndjson(lines);
        }

        /// A search in a point in time, page after page. No index in the URL: the point in time
        /// has them. No `filter_path` either: the pager needs every field it reads.
        POST search_pit(query: Value, pit: Pit, size?: u32, search_after?: Vec<Value>) -> SearchPage {
            path "_search";
            body json {
                query; pit; size; search_after;
                "sort" = ["_shard_doc"];
                "track_total_hits" = false;
            }
            paginate pit_after(pit, search_after);
            idempotent;
        }
        DELETE close_pit(id: String) -> ClosedPit {
            path "_pit";
            body json { id; }
        }

        /// A text table, or JSON rows with `.format(CatFormat::Json)`.
        GET cat_indices(format?: CatFormat) -> CatReply {
            path "_cat/indices";
            query { format; }
            read Text;
            status { 2xx { body } => cat_reply(body, format)? }
        }
    }
}
```

A document, a query and the settings of an index are `serde_json::Value`: each deployment has its own mappings. Four writes say `idempotent false;`, because a second try is not the same as the first: `put` and `delete_document` give the document a new version each time, and an index created or deleted twice gets an error the second time (`400`: it exists; `404`: it is gone), which hides that the first try worked.

## Beside the description

The data types are plain `serde` types, named here and not printed: `GetReply` (`_id`, `found`, `_source`), `WriteReply` (`_id`, `result`), `IndexAck` (`acknowledged`), `CountReply` (`count`), `SearchPage` (`timed_out`, `_shards`, `hits`, `pit_id`), `Hits` and `Hit` (`_id`, `_source`, `sort`), `OpenedPit` (`id`), `ClosedPit`, `BulkReply` (`errors`, `items`: for each line, a map from its action to a `BulkItem` with `status` and `error`), and `Refresh` (`true`, `false`, `wait_for`).

How Elasticsearch says no, the body of many lines, the missing document, the pager of a point in time, the `_cat` table, and a search sent as a `POST`:

```rust
use concord::page::{Call, Slot};
use concord::{reqwest, Answer, Body, Fail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How Elasticsearch says no: `{"error": {"type": …, "reason": …}, "status": 404}`.
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

/// A `404` with `"found": false` is a missing document, an answer; with `"error"`, it is a refusal.
pub fn document_404<T: DeserializeOwned>(body: Value) -> Result<T, Fail<EsProblem>> {
    if body.get("error").is_some() {
        let problem: EsProblem = serde_json::from_value(body)
            .map_err(|_| concord::mismatch("not an Elasticsearch error"))?;
        return Err(problem.into());
    }
    serde_json::from_value(body).map_err(|_| concord::mismatch("not a missing document"))
}

/// The next page of a search in a point in time: the newest id of the point in time, and the
/// sort values of the last hit. An empty page is the end. A page that timed out, or that some
/// shards did not answer, ends the pages with a Mismatch: going on would skip documents.
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

/// The arm of `cat_indices`: the text as it came, or the rows when JSON was asked for.
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
```

## Calling it

```rust
use concord::{basic, Base};
use serde_json::json;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client with a user and a password; a document added, then read back.
pub async fn add_and_get() -> Result<(), BoxError> {
    let url: Base = std::env::var("ES_URL")?.parse()?;
    let es = Elasticsearch::new(url, basic("elastic", std::env::var("ES_PASSWORD")?));
    // With an API key instead: Secret::new(format!("ApiKey {key}")).
    let books = es.index("books");
    let added = books.add(json!({ "title": "Dune" })).refresh(Refresh::WaitFor).await?;
    let got = books.get(&added.id).await?;
    println!("{}: found {}", got.id, got.found);
    Ok(())
}

/// Many writes at once. The answer is a `200` even when some of them failed: look at each one.
pub async fn bulk(es: &Elasticsearch) -> elasticsearch::Result<()> {
    let reply = es.bulk(vec![
        json!({ "index": { "_index": "books", "_id": "42" } }),
        json!({ "title": "Dune" }),
        json!({ "delete": { "_index": "books", "_id": "old" } }),
    ]).await?;
    for item in reply.items {
        for (action, result) in item {
            if let Some(error) = result.error {
                eprintln!("{action}: {} {:?}", error.kind, error.reason);
            }
        }
    }
    Ok(())
}

/// Every hit of a point in time, page after page; then the point in time is closed, also after
/// an error.
pub async fn walk(es: &Elasticsearch) -> elasticsearch::Result<()> {
    let opened = es.indices("logs-*,metrics-*").open_pit("1m").await?;
    let pit = Pit { id: opened.id.clone(), keep_alive: "1m".into() };
    let mut pages = es.search_pit(json!({ "match_all": {} }), pit).size(500).pages();
    let mut latest = opened.id;
    let walked = async {
        while let Some(page) = pages.try_next().await? {
            latest = page.pit_id.clone().unwrap_or(latest.clone());
            for hit in page.hits.hits {
                println!("{}", hit.id);
            }
        }
        Ok::<_, elasticsearch::Error>(())
    }
    .await;
    es.close_pit(latest).await?;
    walked
}

/// A search with a `GET` body, the same as a `POST`, and the `_cat` table as JSON rows.
pub async fn search(es: &Elasticsearch) -> elasticsearch::Result<()> {
    let books = es.indices("books,magazines-*");
    let request = json!({ "query": { "match": { "title": "dune" } }, "size": 5 });
    let first = books.search(&request).await?;
    let same = books.search(request).post().await?;
    println!("{} and {} hits", first.hits.hits.len(), same.hits.hits.len());
    if let CatReply::Json(rows) = es.cat_indices().format(CatFormat::Json).await? {
        println!("{} indices", rows.len());
    }
    Ok(())
}
```

## What fits badly, and the way out

| Elasticsearch wants | What to write |
|---|---|
| `filter_path`, which takes fields out of the answer. The type after the arrow is fixed, and may then miss a field. | Keep the fields that the type needs; for any other shape, `send_raw()`, which gives the `reqwest::Response` (no arm, no typed refusal). |
| A search as a `GET` or as a `POST`, the same operation. One endpoint has one method. | The `GET`, and a method of yours that changes it: `.post()`, with `edit_request` (see [when Concord does not say what you need](../guide/extending.md)). |
| A `404` with two bodies: a missing document (an answer) or a missing index (a refusal). | An arm on the `404` that reads the body and gives one or the other: `document_404`. |
| A point in time to open, walk and close: a lifetime over several calls. | A function of yours (`walk` above) that closes it after the walk, also after an error; the server lets it expire otherwise. |
| A body of many JSON lines (NDJSON). | A writer of yours, `ndjson`, named after `body`. |
| A `200` for a bulk write in which some lines failed. | The answer holds a result for each line: look at each `error`. Nothing is sent again by itself. |
| `_cat` as text, or as JSON rows with `format=json`. The type after the arrow is fixed. | One type for both (`CatReply`), and an arm that reads the text and parses it when JSON was asked for. |
| `?pretty`, a key with no value. | `pretty?: bool`: `.pretty(true)` sends `pretty=true`, which Elasticsearch takes. |
