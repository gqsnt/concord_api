//! Incremental request and answer checks for the generated tour clients.
extern crate tour_gen as tour;

#[tokio::test]
async fn c3_10_generated_tasks_builds_and_reads_without_transport() {
    let tasks = tour::Tasks::new();
    let request = tasks.list().done(false).request().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://tasks.example.com/api/tasks?done=false"
    );
    assert_eq!(tasks.get(7).request().unwrap().url().path(), "/api/tasks/7");
    let task = tasks
        .get(7)
        .read_response(concord::reqwest::Response::from(
            concord::http::Response::new(r#"{"id":7,"title":"generated","done":false}"#),
        ))
        .await
        .unwrap();
    assert_eq!(task.id, 7);
    assert_eq!(
        tasks
            .create(tour::NewTask {
                title: "generated".into()
            })
            .request()
            .unwrap()
            .method(),
        concord::http::Method::POST
    );
    tasks
        .delete(7)
        .read_response(concord::reqwest::Response::from(
            concord::http::Response::new(""),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn c8_6_generated_feed_cursor_pages_run_the_shipped_pager() {
    let feed = tour::Feed::new().with_transport(concord::fake(|_| {
        Ok(concord::reqwest::Response::from(
            concord::http::Response::new(r#"{"data":[],"has_more":false}"#),
        ))
    }));
    assert_eq!(
        feed.by_cursor()
            .starting_after("next")
            .request()
            .unwrap()
            .url()
            .query(),
        Some("starting_after=next")
    );
    let mut pages = feed.by_cursor().pages();
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_none());
}

#[tokio::test]
async fn c5_2_generated_riot_preserves_scopes_and_the_option_arm() {
    let riot = tour::Riot::new("key");
    let request = riot
        .matches(tour::Europe)
        .ids(tour::Puuid::from("p"))
        .start(2)
        .count(5)
        .request()
        .unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://europe.api.riotgames.com/lol/match/v5/matches/by-puuid/p/ids?start=2&count=5"
    );
    assert!(request.headers()["x-riot-token"].is_sensitive());
    let response = concord::http::Response::builder()
        .status(404)
        .body("")
        .unwrap();
    assert!(riot
        .account(tour::Europe)
        .by_riot_id("name", "tag")
        .read_response(response.into())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn c2_14_generated_body_parts_and_files_secrets_and_head() {
    // moves Files::search to `body json { text; cursor; }`, covered by
    // the unchanged tour suite. Keep this separate body-composition contract
    // and its original expected bytes using its own authored description.
    use concord::json;
    concord::api! {
        client BodyParts {
            base "https://body-parts.test";
            part search_cursor(cursor?: String) { body json { cursor; } }
            POST search(text: String, ..search_cursor) {
                body json { "query" = text; "k" = Some(cursor); }
            }
        }
    }
    let request = BodyParts::new().search("needle").request().unwrap();
    assert_eq!(
        request.body().unwrap().as_bytes().unwrap(),
        br#"{"query":"needle","k":null}"#
    );
    let files = tour::Files::new("key");
    let request = files
        .grant("client_credentials", "client", "a&b")
        .request()
        .unwrap();
    assert_eq!(
        request.body().unwrap().as_bytes().unwrap(),
        b"grant_type=client_credentials&client_id=client&client_secret=a%26b"
    );
    let response = concord::http::Response::builder()
        .status(404)
        .body("")
        .unwrap();
    assert!(!files
        .exists(tour::DocId::from("absent"))
        .read_response(response.into())
        .await
        .unwrap());
}
