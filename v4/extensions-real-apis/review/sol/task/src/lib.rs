use concord::Json;
use concord::csv::{Csv, Delimiter, HasHeader, NoHeader, Semicolon};
use serde::Deserialize;

type Export = Csv<HasHeader, Semicolon>;

#[derive(Debug, Deserialize, PartialEq)]
pub struct Row {
    pub id: u32,
    pub name: String,
}
#[derive(Debug, Deserialize)]
pub struct Listing {
    pub items: Vec<u32>,
    pub next: Option<String>,
}

concord::api! {
    pub client ReviewApi {
        base "http://127.0.0.1";
        GET pipe() -> Vec<Row> { path "pipe"; response Csv<NoHeader, Delimiter<b'|'>>; }
        GET export() -> Vec<Row> { path "export"; response Export; }
        GET lines() { path "lines"; }
        GET listing(query cursor?: String) -> Listing { path "listing"; response Json; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord::jsonl::{JsonLinesOptions, JsonLinesReader, RecordErrors};
    use concord_example_testserver::{Body, Reply, TestServer};
    use concord_pagination::{Budget, Outcome, Page, Resume, resume_items, sequential};
    use std::num::NonZeroUsize;
    use std::time::Duration;

    #[tokio::test]
    async fn caller_task() {
        let server = TestServer::start(|r| match r.path() {
            "/pipe" => Reply::text(200, "text/csv; header=absent", "1|alpha\n2|beta\n"),
            "/export" => Reply::text(200, "text/csv; header=present", "id;name\n3;gamma\n"),
            "/lines" => Reply::new(200)
                .header("content-type", "application/jsonl")
                .with_body(Body::Chunks(vec![
                    (Duration::ZERO, "{\"id\":4,\"name\":\"del".into()),
                    (Duration::from_millis(2), "ta\"}\nnot-json\n".into()),
                    (
                        Duration::from_millis(2),
                        "{\"id\":5,\"name\":\"epsilon\"}\n".into(),
                    ),
                ])),
            "/listing" => match r.query_all("cursor").first().map(String::as_str) {
                None => Reply::json(200, r#"{"items":[10,11,12],"next":"opaque+/=& cursor"}"#),
                Some("opaque+/=& cursor") => Reply::json(200, r#"{"items":[13,14],"next":null}"#),
                _ => Reply::json(400, "{}"),
            },
            _ => Reply::new(404),
        })
        .await;
        let api = ReviewApi::new(concord::reqwest::Client::new())
            .with_base_url(server.url())
            .unwrap();
        assert_eq!(
            api.pipe().send().await.unwrap(),
            vec![
                Row {
                    id: 1,
                    name: "alpha".into()
                },
                Row {
                    id: 2,
                    name: "beta".into()
                }
            ]
        );
        assert_eq!(
            api.export().send().await.unwrap(),
            vec![Row {
                id: 3,
                name: "gamma".into()
            }]
        );
        let response = api.lines().prepare().unwrap().send_raw().await.unwrap();
        let reader =
            JsonLinesReader::new(JsonLinesOptions::json_lines().record_errors(RecordErrors::Yield));
        let mut rows = reader.open::<Row>(response).unwrap();
        let (mut good, mut skipped, mut terminal) = (0, 0, 0);
        while let Some(row) = rows.next().await {
            match row {
                Ok(_) => good += 1,
                Err(e) if e.is_terminal() => terminal += 1,
                Err(_) => skipped += 1,
            }
        }
        assert_eq!((good, skipped, terminal), (2, 1, 0));
        let budget = Budget::new(NonZeroUsize::new(10).unwrap());
        let fetch = |cursor: Option<String>| {
            let mut request = api.listing();
            if let Some(cursor) = cursor {
                request = request.set_cursor(cursor);
            }
            async move {
                let page = request.send().await?;
                Ok::<_, concord::Error>(Page::new(page.items, page.next))
            }
        };
        let mut items = sequential(Resume::Start, budget, fetch).items();
        assert_eq!(items.next().await.unwrap().unwrap(), 10);
        let checkpoint = items.checkpoint().unwrap().clone();
        assert_eq!(checkpoint.skip, 1);
        assert_eq!(items.finish().outcome, Outcome::Stopped);
        let mut items = resume_items(checkpoint, budget, fetch);
        let mut rest = Vec::new();
        while let Some(item) = items.next().await {
            rest.push(item.unwrap());
        }
        assert_eq!(rest, [11, 12, 13, 14]);
        assert_eq!(items.finish().outcome, Outcome::Exhausted);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}
