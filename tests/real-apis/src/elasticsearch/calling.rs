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

/// Many writes at once. The response is a `200` even when some of them failed: look at each one.
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
