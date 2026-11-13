//! Socket-free costs of tour clients, paired with handwritten reqwest requests.

mod support;

use concord_core::transport::{Transport, TryInfo};
use concord_testkit::Script;
use reqwest::{Method, Request, Url};
use std::time::Duration;
use tour::{Europe, Feed, Files, Riot, Tasks};

const ACCOUNT: &str = r#"{"puuid":"p-1","gameName":"Ada","tagLine":"EUW"}"#;
const TASK: &str = r#"{"id":7,"title":"bench","done":false}"#;
const PAIRS: [(&str, &str); 3] = [("start", "0"), ("count", "100"), ("queue", "420")];

fn get_request() -> Request {
    let mut url = Url::parse("https://europe.api.riotgames.com/riot/account/v1/accounts/")
        .expect("fixture URL");
    url.path_segments_mut()
        .expect("fixture base")
        .pop_if_empty()
        .extend(["by-riot-id", "Ada", "EUW"]);
    url.query_pairs_mut().extend_pairs(PAIRS);
    let mut request = Request::new(Method::GET, url);
    request
        .headers_mut()
        .insert("x-riot-token", "bench-key".parse().expect("fixture header"));
    request
}

fn post_request() -> Request {
    #[derive(serde::Serialize)]
    struct Entries<'a> {
        text: &'a str,
        cursor: &'a str,
    }
    let mut request = Request::new(
        Method::POST,
        "https://files.example.com/search"
            .parse()
            .expect("fixture URL"),
    );
    request.headers_mut().insert(
        "authorization",
        "Bearer bench-key".parse().expect("fixture header"),
    );
    request
        .headers_mut()
        .insert("x-client", "tour".parse().expect("fixture header"));
    request.headers_mut().insert(
        "content-type",
        "application/json".parse().expect("fixture header"),
    );
    *request.body_mut() = Some(
        serde_json::to_vec(&Entries {
            text: "needle",
            cursor: "next",
        })
        .expect("fixture JSON")
        .into(),
    );
    request
}

fn signed_post_request() -> Request {
    let mut request = post_request();
    // Match tour::Signed on the wire, without using Concord's middleware chain.
    let text = format!("{} {}", request.method(), request.url().path());
    let checksum = b"bench-key"
        .iter()
        .copied()
        .chain([0])
        .chain(text.bytes())
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    let mut value = reqwest::header::HeaderValue::from_str(&format!("{checksum:016x}"))
        .expect("fixture signature");
    value.set_sensitive(true);
    request.headers_mut().insert("x-signature", value);
    request
}

fn main() {
    let mut c = support::criterion();
    let riot = Riot::new("bench-key");
    let get = || {
        riot.account(Europe)
            .by_riot_id("Ada", "EUW")
            .edit_request(|request| {
                request.url_mut().query_pairs_mut().extend_pairs(PAIRS);
            })
    };
    // Verify both fixtures describe exactly the same method, URL and header values.
    let concord = get().request().expect("Concord fixture");
    let hand = get_request();
    assert_eq!(concord.method(), hand.method());
    assert_eq!(concord.url(), hand.url());
    assert_eq!(concord.headers(), hand.headers());
    support::bench(
        &mut c,
        "legacy/get/build/concord",
        || (),
        |()| async { get().request().expect("build") },
    );
    support::bench(
        &mut c,
        "legacy/get/build/reqwest",
        || (),
        |()| async { get_request() },
    );
    support::bench(
        &mut c,
        "legacy/get/send/concord",
        || {
            let script = Script::new();
            script.answer(200).body(ACCOUNT);
            riot.clone().with_transport(script)
        },
        |client| async move {
            client
                .account(Europe)
                .by_riot_id("Ada", "EUW")
                .edit_request(|request| {
                    request.url_mut().query_pairs_mut().extend_pairs(PAIRS);
                })
                .await
                .expect("GET")
        },
    );
    support::bench(
        &mut c,
        "legacy/get/send/reqwest",
        || {
            let script = Script::new();
            script.answer(200).body(ACCOUNT);
            script
        },
        |script| async move {
            let response = script
                .send(get_request(), TryInfo::new("riot.account.by_riot_id", 1))
                .await
                .expect("GET");
            serde_json::from_slice::<tour::Account>(&response.bytes().await.expect("body"))
                .expect("account")
        },
    );
    let files = Files::new("bench-key");
    let post = files
        .search("needle")
        .cursor("next")
        .request()
        .expect("POST fixture");
    let hand = post_request();
    assert_eq!(post.url(), hand.url());
    assert_eq!(post.headers(), hand.headers());
    assert_eq!(
        post.body().and_then(reqwest::Body::as_bytes),
        hand.body().and_then(reqwest::Body::as_bytes)
    );
    support::bench(
        &mut c,
        "post/send/concord",
        || {
            let script = Script::new();
            script.answer(200).body(r#"{"data":[],"next_cursor":null}"#);
            files.clone().with_transport(script)
        },
        |client| async move { client.search("needle").cursor("next").await.expect("POST") },
    );
    support::bench(
        &mut c,
        "post/send/reqwest",
        || {
            let script = Script::new();
            script.answer(200).body(r#"{"data":[],"next_cursor":null}"#);
            script
        },
        |script| async move {
            let response = script
                .send(signed_post_request(), TryInfo::new("files.search", 1))
                .await
                .expect("POST");
            serde_json::from_slice::<tour::Page>(&response.bytes().await.expect("body"))
                .expect("page")
        },
    );
    let tasks = Tasks::new().with_retry(
        concord_core::Retry::none()
            .tries(3)
            .when(|_| Some(Duration::ZERO)),
    );
    support::bench(
        &mut c,
        "retry3/concord",
        || {
            let script = Script::new();
            script.answer(503);
            script.answer(503);
            script.answer(200).body(TASK);
            tasks.clone().with_transport(script)
        },
        |client| async move { client.get(7).await.expect("third try") },
    );
    support::bench(
        &mut c,
        "retry3/reqwest",
        || {
            let script = Script::new();
            script.answer(503);
            script.answer(503);
            script.answer(200).body(TASK);
            script
        },
        |script| async move {
            let request = Request::new(
                Method::GET,
                "https://tasks.example.com/api/tasks/7"
                    .parse()
                    .expect("fixture URL"),
            );
            for number in 1..=3 {
                let response = script
                    .send(
                        request.try_clone().expect("replayable"),
                        TryInfo::new("tasks.get", number),
                    )
                    .await
                    .expect("try");
                if response.status().is_success() {
                    return serde_json::from_slice::<tour::Task>(
                        &response.bytes().await.expect("body"),
                    )
                    .expect("task");
                }
            }
            panic!("fixture must succeed")
        },
    );
    let feed = Feed::new();
    support::bench(
        &mut c,
        "pages10/concord",
        || {
            let script = pages_script();
            feed.clone().with_transport(script)
        },
        |client| async move {
            let mut pages = client.list().pages();
            let mut count = 0;
            while pages.try_next().await.expect("page").is_some() {
                count += 1;
            }
            assert_eq!(count, 10);
            count
        },
    );
    support::bench(
        &mut c,
        "pages10/reqwest",
        pages_script,
        |script| async move {
            let mut cursor: Option<String> = None;
            let mut count = 0;
            loop {
                let mut url = Url::parse("https://feed.example.com/items").expect("fixture URL");
                if let Some(cursor) = cursor.as_deref() {
                    url.query_pairs_mut().append_pair("starting_after", cursor);
                }
                let response = script
                    .send(Request::new(Method::GET, url), TryInfo::new("feed.list", 1))
                    .await
                    .expect("page");
                let page: tour::Page =
                    serde_json::from_slice(&response.bytes().await.expect("body")).expect("page");
                count += 1;
                match page.next_cursor {
                    Some(next) => cursor = Some(next),
                    None => break,
                }
            }
            assert_eq!(count, 10);
            count
        },
    );
    c.final_summary();
}

fn pages_script() -> Script {
    let script = Script::new();
    for page in 0..10 {
        let cursor = (page < 9).then(|| (page + 1).to_string());
        script.answer(200).body(
            serde_json::to_vec(&serde_json::json!({"data":[], "next_cursor":cursor}))
                .expect("fixture JSON"),
        );
    }
    script
}
