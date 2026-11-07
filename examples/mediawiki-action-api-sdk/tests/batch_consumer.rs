//! Consumer test for the batch content fetch, written against the README and rustdoc.
//!
//! An "offline reading pack" application: search, add the user's bookmarks (one of them deleted
//! on the wiki, one repeated), fetch every page's content in one batch and render one line per
//! requested ID. A second case chunks a long bookmark list by the documented page-ID limit.
//! Every response is a synthetic loopback fixture; live provider calls: NOT_RUN.

use std::time::Duration;

use concord_sdk_mediawiki_action_api::{
    MediaWiki, MediaWikiConfig, PageAnswer, PageIdLimit, PageIds, TooManyPageIds, Unavailable,
    concord::reqwest,
};
use concord_testserver::{Recorded, Reply, TestServer};

const UA: &str = "OfflineReadingPack/0.1 (https://example.invalid/pack; pack@example.invalid)";

fn client(server: &TestServer) -> MediaWiki {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    MediaWiki::new(
        http,
        MediaWikiConfig {
            user_agent: UA.into(),
        },
    )
    .with_base_url(format!("{}/w/api.php", server.url()))
    .unwrap()
}

fn one(req: &Recorded, name: &str) -> Option<String> {
    let all = req.query_all(name);
    assert!(all.len() <= 1, "{name} sent {} times", all.len());
    all.into_iter().next()
}

/// The application's rendering of one answer.
fn line(answer: &PageAnswer) -> String {
    match &answer.content {
        Ok(page) => format!(
            "{} r{}: {} bytes",
            page.title,
            page.revision,
            page.content.len()
        ),
        Err(Unavailable::Missing) => format!("#{}: deleted", answer.pageid),
        Err(Unavailable::Invalid) => format!("#{}: invalid", answer.pageid),
        Err(Unavailable::NoContent) => format!("#{}: hidden", answer.pageid),
    }
}

fn optics_wiki(req: &Recorded) -> Reply {
    if one(req, "list").as_deref() == Some("search") {
        return Reply::json(
            200,
            r#"{"batchcomplete":true,"query":{"search":[
                {"ns":0,"title":"Synthetic optics","pageid":101},
                {"ns":0,"title":"Synthetic lenses","pageid":102},
                {"ns":0,"title":"Synthetic mirrors","pageid":103}]}}"#,
        );
    }
    assert_eq!(one(req, "pageids").as_deref(), Some("101|102|103|999"));
    assert_eq!(one(req, "rvlimit"), None);
    let rev = |id: u64, text: &str| {
        format!(
            r#""revisions":[{{"revid":{id}0,"slots":{{"main":{{"contentmodel":"wikitext","content":"{text}"}}}}}}]"#
        )
    };
    match (
        one(req, "rvcontinue").as_deref(),
        one(req, "continue").as_deref(),
    ) {
        (None, None) => Reply::json(
            200,
            format!(
                r#"{{"continue":{{"rvcontinue":"102|5","continue":"||"}},"query":{{"pages":[
                {{"pageid":101,"ns":0,"title":"Synthetic optics",{}}},
                {{"pageid":102,"ns":0,"title":"Synthetic lenses"}},
                {{"pageid":103,"ns":0,"title":"Synthetic mirrors",{}}},
                {{"pageid":999,"missing":true}}]}}}}"#,
                rev(101, "Optics."),
                rev(103, "Mirrors reflect.")
            ),
        ),
        (Some("102|5"), Some("||")) => Reply::json(
            200,
            format!(
                r#"{{"batchcomplete":true,"query":{{"pages":[
                {{"pageid":101,"ns":0,"title":"Synthetic optics"}},
                {{"pageid":102,"ns":0,"title":"Synthetic lenses",{}}},
                {{"pageid":103,"ns":0,"title":"Synthetic mirrors"}},
                {{"pageid":999,"missing":true}}]}}}}"#,
                rev(102, "Lenses refract light.")
            ),
        ),
        _ => Reply::json(
            200,
            r#"{"error":{"code":"badcontinue","info":"Invalid continue param."}}"#,
        ),
    }
}

#[tokio::test]
async fn reading_pack_fetches_hits_and_bookmarks_in_one_continued_batch() {
    let server = TestServer::start(optics_wiki).await;
    let query = client(&server).query();
    let hits = query.search("Synthetic optics").send().await.unwrap();
    let bookmarks = [999, 102];
    let wanted = hits
        .data
        .search
        .iter()
        .map(|hit| hit.pageid)
        .chain(bookmarks);
    let ids = PageIds::new(wanted, PageIdLimit::Standard).unwrap();
    let pack: Vec<String> = query
        .fetch_pages(&ids)
        .await
        .unwrap()
        .iter()
        .map(line)
        .collect();
    assert_eq!(
        pack,
        [
            "Synthetic optics r1010: 7 bytes",
            "Synthetic lenses r1020: 21 bytes",
            "Synthetic mirrors r1030: 16 bytes",
            "#999: deleted",
            "Synthetic lenses r1020: 21 bytes",
        ]
    );
    let reqs = server.requests();
    assert_eq!(reqs.len(), 3, "one search, two content requests");
    for req in &reqs {
        assert_eq!(req.header("user-agent"), Some(UA));
        assert!(req.header_all("authorization").is_empty());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn long_bookmark_lists_are_chunked_by_the_documented_limit() {
    let server = TestServer::start(|req: &Recorded| {
        let ids = one(req, "pageids").unwrap();
        let rows: Vec<String> = ids
            .split('|')
            .map(|id| format!(r#"{{"pageid":{id},"missing":true}}"#))
            .collect();
        Reply::json(
            200,
            format!(
                r#"{{"batchcomplete":true,"query":{{"pages":[{}]}}}}"#,
                rows.join(",")
            ),
        )
    })
    .await;
    let query = client(&server).query();
    let bookmarks: Vec<u64> = (1..=60).collect();

    let refused = PageIds::new(bookmarks.iter().copied(), PageIdLimit::Standard).unwrap_err();
    assert_eq!(
        refused,
        TooManyPageIds {
            count: 60,
            limit: 50
        }
    );
    assert!(server.requests().is_empty(), "refused before sending");

    let mut answered = Vec::new();
    for chunk in bookmarks.chunks(PageIdLimit::Standard.max()) {
        let ids = PageIds::new(chunk.iter().copied(), PageIdLimit::Standard).unwrap();
        answered.extend(query.fetch_pages(&ids).await.unwrap());
    }
    assert_eq!(
        answered.iter().map(|a| a.pageid).collect::<Vec<_>>(),
        bookmarks
    );
    assert!(
        answered
            .iter()
            .all(|a| a.content == Err(Unavailable::Missing))
    );
    let reqs = server.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(one(&reqs[0], "pageids").unwrap().split('|').count(), 50);
    assert_eq!(one(&reqs[1], "pageids").unwrap().split('|').count(), 10);
    server.shutdown().await;
}
