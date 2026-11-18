//! C8.6: a next-page URL is sent as the server gave it, without its fragment. A URL with an empty
//! query (`/slice?`, `?&&`) is not the URL without one, in what is sent and in the comparison with
//! the request just sent. A call given a URL with `at_url` goes out the same way.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::page::IntoItems;
use concord::{by_link, by_next_url, fake, reply_with, Json};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// A page of a list that names its next page.
#[derive(Debug, Deserialize)]
pub struct Users {
    /// The items.
    pub value: Vec<String>,
    /// The URL of the next page.
    #[serde(rename = "@odata.nextLink")]
    pub next: Option<String>,
}
impl IntoItems for Users {
    type Item = String;
    fn into_items(self) -> Vec<String> {
        self.value
    }
    fn count(&self) -> usize {
        self.value.len()
    }
}

concord::api! {
    client Graph {
        base "https://api.test";
        read Json;
        GET users() -> Users {
            path "v1.0/users";
            paginate by_next_url(|page: &Users| page.next.clone());
        }
        GET linked() -> Users {
            path "v1.0/users";
            paginate by_link();
        }
        GET user() -> Users { path "v1.0/users"; }
    }
}

/// A transport that records each URL sent, and answers by the URL: the first page names `next`
/// (in the body, and in a `Link` field); the page at `/slice?` and the page at `/slice` differ.
fn server(next: &'static str) -> (Graph, Arc<Mutex<Vec<String>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let log = sent.clone();
    let transport = fake(move |request| {
        let url = request.url();
        log.lock().unwrap().push(url.to_string());
        let first = log.lock().unwrap().len() == 1;
        let body = match (first, url.path(), url.query()) {
            (true, _, _) => format!(r#"{{"value":["1"],"@odata.nextLink":"{next}"}}"#),
            (false, "/slice", None) => r#"{"value":["wrong"]}"#.to_owned(),
            (false, _, _) => r#"{"value":["2"]}"#.to_owned(),
        };
        let link = format!("<{next}>; rel=next");
        let headers: Vec<(&str, &str)> = if first {
            vec![
                ("content-type", "application/json"),
                ("link", link.as_str()),
            ]
        } else {
            vec![("content-type", "application/json")]
        };
        Ok(reply_with(200, &headers, body))
    });
    (Graph::new().with_transport(transport), sent)
}

async fn walk(mut items: concord::page::Items<String>) -> Result<Vec<String>, concord::Error> {
    let mut all = Vec::new();
    while let Some(item) = items.try_next().await? {
        all.push(item);
    }
    Ok(all)
}

#[tokio::test(start_paused = true)]
async fn c8_6_a_next_url_with_an_empty_query_is_sent_with_it() {
    for linked in [false, true] {
        let (graph, sent) = server("/slice?");
        let pages = if linked {
            graph.linked().pages()
        } else {
            graph.users().pages()
        };
        let items = walk(pages.items()).await.unwrap();
        assert_eq!(items, ["1", "2"], "linked: {linked}");
        assert_eq!(
            *sent.lock().unwrap(),
            ["https://api.test/v1.0/users", "https://api.test/slice?"],
            "linked: {linked}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_an_empty_query_on_the_current_path_is_not_the_request_just_sent() {
    for next in ["?", "?&&"] {
        for linked in [false, true] {
            let (graph, sent) = server(next);
            let pages = if linked {
                graph.linked().pages()
            } else {
                graph.users().pages()
            };
            let items = walk(pages.items()).await.unwrap();
            assert_eq!(items, ["1", "2"], "{next}, linked: {linked}");
            assert_eq!(
                *sent.lock().unwrap(),
                [
                    "https://api.test/v1.0/users".to_owned(),
                    format!("https://api.test/v1.0/users{next}")
                ],
                "{next}, linked: {linked}"
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_a_next_url_without_a_query_or_with_a_fragment_is_sent_as_given() {
    for (next, expected) in [
        ("/slice", "https://api.test/slice"),
        ("/slice?a=1#part", "https://api.test/slice?a=1"),
        ("/slice?#part", "https://api.test/slice?"),
    ] {
        let (graph, sent) = server(next);
        walk(graph.users().pages().items()).await.unwrap();
        assert_eq!(sent.lock().unwrap()[1], expected, "{next}");
    }
}

/// The same builder makes a call given a URL with `at_url`, outside pages: each form goes out as
/// given, without its fragment. A call without `at_url` has no query.
#[tokio::test(start_paused = true)]
async fn c8_6_a_call_given_a_url_sends_its_query_as_given() {
    use concord::page::Call;
    for (url, expected) in [
        ("/slice", "https://api.test/slice"),
        ("/slice?", "https://api.test/slice?"),
        ("/slice?&&", "https://api.test/slice?&&"),
        ("/slice?a=1&&b=2", "https://api.test/slice?a=1&&b=2"),
        ("/slice?#part", "https://api.test/slice?"),
    ] {
        let (graph, sent) = server("/unused");
        graph.user().at_url(url).await.unwrap();
        assert_eq!(sent.lock().unwrap()[0], expected, "{url}");
    }
    let (graph, sent) = server("/unused");
    graph.user().await.unwrap();
    assert_eq!(sent.lock().unwrap()[0], "https://api.test/v1.0/users");
}

/// An `edit_request` that moves the host is not combined with a pager that uses `at_url` (C8.6).
/// What each case does, for the reason an author can read in C8.6: a next URL is checked against
/// the origin the call builds from its base, before the edits; a relative one is first resolved
/// against the URL of the response, which the edit had moved.
#[tokio::test(start_paused = true)]
async fn c8_6_a_host_moving_edit_with_a_next_url_checks_the_origin_the_call_builds() {
    for (next, second) in [
        ("https://api.test/slice", Some("https://moved.test/slice")),
        ("/slice", None),
    ] {
        let (graph, sent) = server(next);
        let mut pages = graph
            .users()
            .edit_request(|request| {
                let _ = request.url_mut().set_host(Some("moved.test"));
            })
            .pages();
        assert!(pages.try_next().await.unwrap().is_some());
        match second {
            Some(url) => {
                assert!(pages.try_next().await.unwrap().is_some(), "{next}");
                assert_eq!(sent.lock().unwrap()[1], url);
            }
            None => {
                let error = pages.try_next().await.unwrap_err();
                assert_eq!(error.kind(), concord::error::Kind::Build, "{next}");
                assert_eq!(sent.lock().unwrap().len(), 1);
            }
        }
    }
}
