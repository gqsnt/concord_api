//! Connected workflows against independently authored fixtures, never the provider.

mod support;

use std::num::NonZeroU32;

use concord_example_testserver::{Recorded, Reply, TestServer};

#[tokio::test]
async fn search_page_select_work_and_decode_missing_optional_fields() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/search.json" => Reply::json(200, include_str!("../fixtures/work-search.json")),
        "/works/OL27448W.json" => Reply::json(200, include_str!("../fixtures/work-minimal.json")),
        _ => Reply::new(500),
    })
    .await;
    let api = support::client(&server);
    let page = api
        .search()
        .works("field notes")
        .set_page(NonZeroU32::new(2).unwrap())
        .set_limit(NonZeroU32::new(20).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(page.num_found, Some(2));
    assert_eq!(page.start, Some(20));
    assert_eq!(page.num_found_exact, Some(false));
    let selected = page.docs.first().expect("fixture has one selectable work");
    assert_eq!(selected.title.as_deref(), Some("Synthetic field notes"));
    assert_eq!(selected.first_publish_year, Some(1954));
    let work = api
        .works()
        .get(selected.id().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(work.key.as_deref(), Some("/works/OL27448W"));
    assert!(work.title.is_none());
    assert!(work.description.is_none());
    assert!(work.subjects.is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].target,
        "/search.json?q=field+notes&page=2&limit=20"
    );
    assert_eq!(requests[1].target, "/works/OL27448W.json");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn search_authors_select_and_retrieve_their_works() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/search/authors.json" => Reply::json(200, include_str!("../fixtures/author-search.json")),
        "/authors/OL23919A/works.json" => {
            Reply::json(200, include_str!("../fixtures/author-works.json"))
        }
        _ => Reply::new(500),
    })
    .await;
    let api = support::client(&server);
    let found = api.search().authors("Fixture Author").send().await.unwrap();
    let author = found.docs.first().expect("fixture has one author");
    assert_eq!(author.name.as_deref(), Some("Fixture Author"));
    assert_eq!(author.work_count, Some(2));
    let works = api
        .author(author.id().unwrap())
        .works()
        .send()
        .await
        .unwrap();
    assert_eq!(works.size, Some(2));
    assert_eq!(works.entries.len(), 2);
    assert_eq!(
        works.entries[0].title.as_deref(),
        Some("Synthetic field notes")
    );
    assert_eq!(
        works.entries[0].description.as_ref().unwrap().as_str(),
        Some("An authored fixture.")
    );
    assert!(works.entries[1].title.is_none());
    assert!(works.entries[1].subjects.is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "provider links are not followed");
    assert_eq!(requests[0].target, "/search/authors.json?q=Fixture+Author");
    assert_eq!(requests[1].target, "/authors/OL23919A/works.json");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn empty_pages_have_no_selection_and_do_not_trigger_a_followup() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/search.json" => Reply::json(200, include_str!("../fixtures/empty-work-search.json")),
        "/search/authors.json" => {
            Reply::json(200, include_str!("../fixtures/empty-author-search.json"))
        }
        _ => Reply::new(500),
    })
    .await;
    let api = support::client(&server);
    let page = api
        .search()
        .works("no matches")
        .set_page(NonZeroU32::new(4).unwrap())
        .send()
        .await
        .unwrap();
    // A reported total does not imply this page has a selected item or a next page.
    assert_eq!(page.num_found, Some(2));
    let selection = match page.docs.first() {
        Some(hit) => Some(api.works().get(hit.id().unwrap()).send().await.unwrap()),
        None => None,
    };
    assert!(selection.is_none());
    let authors = api.search().authors("no matches").send().await.unwrap();
    let collection = match authors.docs.first() {
        Some(hit) => Some(api.author(hit.id().unwrap()).works().send().await.unwrap()),
        None => None,
    };
    assert!(collection.is_none());
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}
