//! The example pages that use `Sse`, `Rotating`, `multipart_related` and `gzip`: their blocks
//! are what this crate compiles, and their calls are run against scripted responses.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use concord_testkit::Script;
use std::{fs, path::PathBuf};

mod rotating {
    use real_apis::github::*;
    include!("../fixtures/examples/github_rotating.rs");
}

#[test]
fn c1_20_the_blocks_of_the_new_example_pages_are_compiled_word_for_word() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for api in ["anthropic", "drive"] {
        let page =
            fs::read_to_string(package.join(format!("../../docs/examples/{api}.md"))).unwrap();
        let mut blocks = 0;
        for entry in fs::read_dir(package.join(format!("src/{api}"))).unwrap() {
            let path = entry.unwrap().path();
            let file = path.file_name().unwrap().to_str().unwrap().to_owned();
            if file == "mod.rs" || file.starts_with("stand_in") {
                continue;
            }
            let code = fs::read_to_string(&path).unwrap();
            assert!(
                page.contains(&format!("```rust\n{}\n```", code.trim())),
                "{api}: the page does not print src/{api}/{file} as one block"
            );
            blocks += 1;
        }
        assert_eq!(
            blocks, 3,
            "{api}: a description, the Rust beside it, the calls"
        );
    }
    let github = include_str!("../../../docs/examples/github.md");
    let code = include_str!("../fixtures/examples/github_rotating.rs");
    assert!(github.contains(&format!("```rust\n{}\n```", code.trim())));
}

const STREAM: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"content\":[]}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: ping\n\
data: {\"type\": \"ping\"}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Nile, \"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Amazon\"}}\n\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

#[tokio::test(start_paused = true)]
async fn c11_9_a_streamed_reply_is_typed_by_the_authors_function() {
    use real_apis::anthropic::{stream, Anthropic, ReplyError};
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "text/event-stream")
        .body(&STREAM.as_bytes()[..40])
        .body(&STREAM.as_bytes()[40..]);
    script
        .answer(200)
        .header("content-type", "text/event-stream")
        .body("event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n");
    script.answer(200).body("event: ping\ndata: {}\n\n");
    let api = Anthropic::new("PRIVATE_API_KEY").with_transport(script.clone());

    let text = stream(&api, "a-model", "Name two rivers.").await.unwrap();
    assert_eq!(text, "Nile, Amazon");
    match stream(&api, "a-model", "again").await.unwrap_err() {
        ReplyError::Api(error) => assert_eq!(error.kind, "overloaded_error"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        stream(&api, "a-model", "again").await.unwrap_err(),
        ReplyError::Unfinished
    ));

    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    let first = &requests[0];
    assert_eq!(
        first.url().as_str(),
        "https://api.anthropic.com/v1/messages"
    );
    assert_eq!(first.header("x-api-key"), Some("PRIVATE_API_KEY"));
    assert_eq!(first.header("anthropic-version"), Some("2023-06-01"));
    let body: serde_json::Value = serde_json::from_slice(first.body().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "model": "a-model",
            "max_tokens": 1024,
            "messages": [{ "role": "user", "content": "Name two rivers." }],
            "stream": true,
        })
    );
}

#[tokio::test(start_paused = true)]
async fn c11_9_an_api_error_before_the_stream_is_the_clients_problem() {
    use real_apis::anthropic::{stream, Anthropic, ReplyError};
    let script = Script::new();
    script
        .answer(400)
        .header("content-type", "application/json")
        .body("{\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"max_tokens\"}}");
    let api = Anthropic::new("PRIVATE_API_KEY").with_transport(script.clone());
    match stream(&api, "a-model", "Hello").await.unwrap_err() {
        ReplyError::Call(error) => {
            assert!(error.is_status(400));
            assert_eq!(error.problem().unwrap().error.kind, "invalid_request_error");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn c12_6_github_renews_a_users_token_and_saves_the_new_pair_first() {
    use concord::auth::{CredentialStore, RotationError, Stored, TokenPair};
    use real_apis::github::{Github, GithubLogin, QuotaLog};
    use rotating::{as_user, work_as_user, FileStore};

    let directory = std::env::temp_dir().join(format!("concord-github-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let store = FileStore(directory.join("user.json"));
    store
        .save(Stored::Ready(TokenPair::new("old-access", "r0", None)))
        .await
        .unwrap();

    let tokens = Script::new();
    tokens
        .answer(200)
        .header("content-type", "application/json")
        .body("{\"access_token\":\"a1\",\"expires_in\":28800,\"refresh_token\":\"r1\",\"token_type\":\"bearer\"}");
    let api = Script::new();
    api.answer(200)
        .header("content-type", "application/json")
        .body("{}");
    let github = Github::new("my-app/1.0", QuotaLog::default()).with_transport(api.clone());
    let login = GithubLogin::new().with_transport(tokens.clone());
    let (github, driver) = as_user(
        github,
        login,
        store.clone(),
        "Iv1.app".into(),
        "PRIVATE_SECRET".into(),
    )
    .unwrap();
    work_as_user(github, driver, store.clone()).await.unwrap();

    let exchanges = tokens.requests();
    assert_eq!(exchanges.len(), 1);
    assert_eq!(
        exchanges[0].url().as_str(),
        "https://github.com/login/oauth/access_token"
    );
    assert_eq!(exchanges[0].header("accept"), Some("application/json"));
    assert_eq!(
        exchanges[0].body().unwrap(),
        b"client_id=Iv1.app&client_secret=PRIVATE_SECRET&refresh_token=r0&grant_type=refresh_token"
    );
    let calls = api.requests();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].url().as_str(), "https://api.github.com/user");
    assert_eq!(calls[0].header("authorization"), Some("Bearer a1"));
    match store.load().await.unwrap() {
        Stored::Ready(pair) => {
            assert_eq!(&*pair.access().expose(), "a1");
            assert_eq!(&*pair.refresh().expose(), "r1");
        }
        Stored::Refreshing => panic!("the new pair was not saved"),
    }

    // A refused refresh: the call fails, the refresh token is not sent twice, and the user
    // must sign in again.
    let tokens = Script::new();
    tokens
        .answer(200)
        .header("content-type", "application/json")
        .body("{\"error\":\"bad_refresh_token\",\"error_description\":\"expired\"}");
    store
        .save(Stored::Ready(TokenPair::new("a1", "r1", None)))
        .await
        .unwrap();
    let api = Script::new();
    let github = Github::new("my-app/1.0", QuotaLog::default()).with_transport(api.clone());
    let login = GithubLogin::new().with_transport(tokens.clone());
    let (github, driver) = as_user(
        github,
        login,
        store.clone(),
        "Iv1.app".into(),
        "PRIVATE_SECRET".into(),
    )
    .unwrap();
    let driver = tokio::spawn(driver);
    let first = github.user().get().await.unwrap_err();
    assert!(RotationError::find(&first).is_some(), "{first}");
    let second = github.user().get().await.unwrap_err();
    assert!(
        matches!(
            RotationError::find(&second),
            Some(RotationError::NeedsReauthorization)
        ),
        "{second}"
    );
    drop(github);
    assert!(matches!(
        driver.await.unwrap(),
        Err(RotationError::Renewal(_))
    ));
    assert_eq!(tokens.requests().len(), 1);
    assert!(api.requests().is_empty());
    fs::remove_dir_all(&directory).unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_3_drive_uploads_metadata_and_content_in_one_related_body() {
    use concord::Body;
    use real_apis::drive::{Drive, NewFile};
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body("{\"id\":\"f1\",\"name\":\"report.csv\"}");
    let drive = Drive::new("PRIVATE_TOKEN").with_transport(script.clone());
    let metadata = NewFile {
        name: Some("report.csv".into()),
        parents: vec!["folder-1".into()],
        ..Default::default()
    };
    let file = drive
        .upload(metadata, Body::new("a,b\n1,2\n", "text/csv"))
        .await
        .unwrap();
    assert_eq!(file.id, "f1");

    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        request.url().as_str(),
        "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart"
    );
    assert_eq!(
        request.header("authorization"),
        Some("Bearer PRIVATE_TOKEN")
    );
    let content_type = request.header("content-type").unwrap();
    let boundary = content_type
        .strip_prefix("multipart/related; type=\"application/json\"; boundary=")
        .unwrap_or_else(|| panic!("{content_type}"));
    let expected = format!(
        "--{boundary}\r\ncontent-type: application/json; charset=UTF-8\r\n\r\n\
         {{\"name\":\"report.csv\",\"parents\":[\"folder-1\"]}}\r\n\
         --{boundary}\r\ncontent-type: text/csv\r\n\r\na,b\n1,2\n\r\n--{boundary}--\r\n"
    );
    assert_eq!(
        std::str::from_utf8(request.body().unwrap()).unwrap(),
        expected
    );
}

#[tokio::test(start_paused = true)]
async fn c12_2_elasticsearch_sends_the_bulk_lines_compressed_with_gzip() {
    use concord::Body;
    use real_apis::elasticsearch::Elasticsearch;
    let script = Script::new();
    script.answer(204);
    let api = Elasticsearch::new(
        "https://search.test".parse::<concord::Base>().unwrap(),
        "PRIVATE_API_KEY",
    )
    .with_transport(script.clone());
    api.bulk_gzip(vec![
        serde_json::json!({"index":{"_id":"42"}}),
        serde_json::json!({"title":"Dune"}),
    ])
    .send_raw()
    .await
    .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.url().as_str(), "https://search.test/_bulk");
    assert_eq!(request.header("content-encoding"), Some("gzip"));
    assert_eq!(request.header("content-type"), Some("application/x-ndjson"));
    let lines = Body::new(
        "{\"index\":{\"_id\":\"42\"}}\n{\"title\":\"Dune\"}\n",
        "application/x-ndjson",
    );
    let compressed = concord::gzip(lines).unwrap();
    assert_eq!(request.body().unwrap(), compressed.as_bytes().unwrap());
    assert_eq!(&request.body().unwrap()[..2], [0x1f, 0x8b]);
}
