//! The ordinary traversals (`items()`, `pages()`: typed end, opt-in cap, kept prefix) and the
//! cursor adapter under `concord_pagination`'s engine: page and item traversal, item-level
//! checkpoints with a mid-page stop and a replayed resume, a backward chain, and budgets.

use std::num::NonZeroUsize;

use concord_example_stripe_sdk::pages::{CursorList, Direction};
use concord_example_stripe_sdk::{
    AccountId, Customer, CustomerId, Limit, ProtocolError, SecretKey, StripeContext,
    StripeCustomersList,
};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome, PagesError, Resume, resume_items, sequential};

const KEY: &str = "sk_test_SYNTHETIC_pages";

fn customer(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","object":"customer","email":null,"name":null,"description":null,"created":1767225600}}"#
    )
}

/// Five customers, newest first: cus_e .. cus_a; `limit=2`. Forward pages follow
/// `starting_after`, backward pages `ending_before`, as the reference describes.
fn respond(request: &Recorded) -> Reply {
    let all = ["cus_e", "cus_d", "cus_c", "cus_b", "cus_a"];
    let after = request.query_all("starting_after");
    let before = request.query_all("ending_before");
    let (page, more): (Vec<&str>, bool) = match (after.first(), before.first()) {
        (None, None) => (all[0..2].to_vec(), true),
        (Some(id), None) => {
            let start = all.iter().position(|c| c == id).unwrap() + 1;
            let end = (start + 2).min(all.len());
            (all[start..end].to_vec(), end < all.len())
        }
        (None, Some(id)) => {
            let end = all.iter().position(|c| c == id).unwrap();
            let start = end.saturating_sub(2);
            (all[start..end].to_vec(), start > 0)
        }
        (Some(_), Some(_)) => {
            return Reply::json(
                400,
                r#"{"error":{"type":"invalid_request_error","code":"parameters_exclusive"}}"#,
            );
        }
    };
    let data: Vec<String> = page.iter().map(|id| customer(id)).collect();
    Reply::json(
        200,
        format!(
            r#"{{"object":"list","url":"/v1/customers","has_more":{more},"data":[{}]}}"#,
            data.join(",")
        ),
    )
}

fn context(server: &TestServer) -> StripeContext {
    StripeContext::new(
        concord::reqwest::Client::new(),
        &SecretKey::new(KEY).unwrap(),
    )
    .with_base_url(format!("{}/v1", server.url()))
    .unwrap()
    .with_account(Some(AccountId::new("acct_ALPHA").unwrap()))
}

fn budget(pages: usize) -> Budget {
    Budget::new(NonZeroUsize::new(pages).unwrap())
}

fn cursors(server: &TestServer) -> Vec<(Vec<String>, Vec<String>)> {
    server
        .requests()
        .iter()
        .map(|r| (r.query_all("starting_after"), r.query_all("ending_before")))
        .collect()
}

#[tokio::test]
async fn items_stop_mid_page_and_resume_by_replaying_the_page() {
    let server = TestServer::start(respond).await;
    let stripe = context(&server);
    let list = CursorList::forward(
        stripe
            .api()
            .customers()
            .list()
            .set_limit(Limit::new(2).unwrap()),
    )
    .unwrap();

    let mut items = sequential(Resume::Start, budget(10), |cursor| list.fetch(cursor)).items();
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(items.next().await.unwrap().unwrap().id);
    }
    // Stopped after the first item of page 2: the checkpoint names page 2 and one item to skip.
    let checkpoint = items.checkpoint().cloned().unwrap();
    assert_eq!(checkpoint.skip, 1);
    assert_eq!(checkpoint.page.key().unwrap().id().as_str(), "cus_d");
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(
        report.remainder.len(),
        1,
        "the rest of page 2 is handed back"
    );

    let mut resumed = resume_items(checkpoint, budget(10), |cursor| list.fetch(cursor));
    while let Some(item) = resumed.next().await {
        seen.push(item.unwrap().id);
    }
    assert_eq!(resumed.finish().outcome, Outcome::Exhausted);
    let ids: Vec<&str> = seen.iter().map(CustomerId::as_str).collect();
    assert_eq!(ids, ["cus_e", "cus_d", "cus_c", "cus_b", "cus_a"]);
    // Page 2 was fetched again (replay policy), then page 3.
    let after: Vec<Vec<String>> = cursors(&server).into_iter().map(|(a, _)| a).collect();
    assert_eq!(
        after,
        [
            vec![],
            vec!["cus_d".to_owned()],
            vec!["cus_d".to_owned()],
            vec!["cus_b".to_owned()]
        ]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn backward_chain_uses_ending_before_from_the_first_id() {
    let server = TestServer::start(respond).await;
    let stripe = context(&server);
    let list = CursorList::new(
        stripe
            .api()
            .customers()
            .list()
            .set_limit(Limit::new(2).unwrap()),
        Direction::Backward,
    )
    .unwrap();
    // Start before the oldest customer and walk towards newer ones.
    let from = Resume::At(list.continue_from(CustomerId::new("cus_a").unwrap()));
    let mut pages = sequential(from, budget(10), |cursor| list.fetch(cursor));
    let mut seen = Vec::new();
    while let Some(page) = pages.next().await {
        let page = page.unwrap();
        seen.push(
            page.items
                .iter()
                .map(|c| c.id.to_string())
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    assert_eq!(
        seen,
        [vec!["cus_c", "cus_b"], vec!["cus_e", "cus_d"]],
        "each page keeps Stripe's newest-first order"
    );
    let before: Vec<Vec<String>> = cursors(&server).into_iter().map(|(_, b)| b).collect();
    assert_eq!(before, [vec!["cus_a".to_owned()], vec!["cus_c".to_owned()]]);
    assert!(cursors(&server).iter().all(|(after, _)| after.is_empty()));
    server.shutdown().await;
}

#[tokio::test]
async fn page_budget_bounds_the_chain_and_keeps_the_next_cursor() {
    let server = TestServer::start(respond).await;
    let stripe = context(&server);
    let list = CursorList::forward(
        stripe
            .api()
            .customers()
            .list()
            .set_limit(Limit::new(2).unwrap()),
    )
    .unwrap();
    let mut pages = sequential(Resume::Start, budget(2), |cursor| list.fetch(cursor));
    let mut count = 0;
    while let Some(page) = pages.next().await {
        count += page.unwrap().items.len();
    }
    let report = pages.finish();
    assert_eq!(count, 4);
    assert_eq!(report.outcome, Outcome::Bounded);
    let Some(Resume::At(next)) = report.resume else {
        panic!("bounded chains keep the next cursor")
    };
    assert_eq!(next.id().as_str(), "cus_b");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[test]
fn page_futures_are_send() {
    fn send<T: Send>(_: &T) {}
    let stripe = StripeContext::new(
        concord::reqwest::Client::new(),
        &SecretKey::new(KEY).unwrap(),
    );
    let list = CursorList::forward(stripe.api().invoices().list()).unwrap();
    let future = list.fetch(None);
    send(&future);
}

/// A generated list of 2,500 customers with pseudo-random ids (seeded linear congruential
/// generator, so ids carry no order or count), served 100 per page. The chain must fetch every
/// object exactly once, in order, with each `starting_after` equal to the previous page's last
/// id: a cursor guessed from a count or an index could not match these ids.
#[tokio::test]
async fn generated_list_is_walked_exactly_once_by_object_cursors() {
    const COUNT: usize = 2_500;
    const SEED: u64 = 0x5EED_2026_1104;
    let mut state = SEED;
    let ids: Vec<String> = (0..COUNT)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            format!("cus_{:016x}", state)
        })
        .collect();
    let position: std::collections::HashMap<String, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    let served = ids.clone();
    let server = TestServer::start(move |request| {
        let limit: usize = request.query_all("limit")[0].parse().unwrap();
        let start = match request.query_all("starting_after").first() {
            None => 0,
            Some(id) => position[id] + 1,
        };
        let end = (start + limit).min(served.len());
        let data: Vec<String> = served[start..end].iter().map(|id| customer(id)).collect();
        Reply::json(
            200,
            format!(
                r#"{{"object":"list","url":"/v1/customers","has_more":{},"data":[{}]}}"#,
                end < served.len(),
                data.join(",")
            ),
        )
    })
    .await;
    let stripe = context(&server);
    let list = CursorList::forward(
        stripe
            .api()
            .customers()
            .list()
            .set_limit(Limit::new(100).unwrap()),
    )
    .unwrap();
    let mut pages = sequential(Resume::Start, budget(1_000), |cursor| list.fetch(cursor));
    let mut seen = Vec::with_capacity(COUNT);
    while let Some(page) = pages.next().await {
        seen.extend(page.unwrap().items.into_iter().map(|c| c.id.to_string()));
    }
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.succeeded, 25);
    assert_eq!(seen, ids);
    let requests = server.requests();
    assert_eq!(requests.len(), 25);
    for (index, request) in requests.iter().enumerate().skip(1) {
        assert_eq!(
            request.query_all("starting_after"),
            [ids[index * 100 - 1].clone()],
            "page {index} continues from the previous page's last id"
        );
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

fn customers(stripe: &StripeContext) -> StripeCustomersList {
    stripe
        .api()
        .customers()
        .list()
        .set_limit(Limit::new(2).unwrap())
}

fn ids(customers: &[Customer]) -> Vec<&str> {
    customers.iter().map(|c| c.id.as_str()).collect()
}

/// The ordinary path: the generated list operation's `items()` reads every page, forward, and
/// ends only after the page with `has_more: false`.
#[tokio::test]
async fn ordinary_items_read_every_page_to_the_end() {
    let server = TestServer::start(respond).await;
    let stripe = context(&server);
    let all = customers(&stripe)
        .items()
        .unwrap()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(ids(&all), ["cus_e", "cus_d", "cus_c", "cus_b", "cus_a"]);

    let mut pages = customers(&stripe).pages().unwrap();
    let mut sizes = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        assert_eq!(page.meta.status, 200);
        sizes.push(page.items.len());
    }
    assert_eq!(sizes, [2, 2, 1]);
    assert!(
        pages.next_page().await.unwrap().is_none(),
        "the end repeats"
    );
    assert_eq!(server.requests().len(), 6);
    server.shutdown().await;
}

/// A cap reached while Stripe has more is an error carrying the next cursor, never a silent
/// prefix; continuing from it reads the rest. A cap reached exactly on the last page is the end.
#[tokio::test]
async fn a_cap_with_more_pages_is_an_error_and_on_the_last_page_is_the_end() {
    let server = TestServer::start(respond).await;
    let stripe = context(&server);
    let two = NonZeroUsize::new(2).unwrap();

    let incomplete = customers(&stripe)
        .items()
        .unwrap()
        .max_pages(two)
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        ids(&incomplete.collected),
        ["cus_e", "cus_d", "cus_c", "cus_b"]
    );
    let PagesError::Bounded { resume, .. } = incomplete.error else {
        panic!("a cap with more pages is `Bounded`: {:?}", incomplete.error)
    };
    let Resume::At(cursor) = &resume else {
        panic!("the resume point is the next cursor")
    };
    assert_eq!(cursor.id().as_str(), "cus_b");
    assert_eq!(server.requests().len(), 2, "the cap is not exceeded");

    let list = CursorList::forward(customers(&stripe)).unwrap();
    let rest = list
        .pages()
        .starting_at(resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(ids(&rest), ["cus_a"]);

    let three = NonZeroUsize::new(3).unwrap();
    let mut pages = customers(&stripe).pages().unwrap().max_pages(three);
    let mut read = 0;
    while let Some(page) = pages.next_page().await.unwrap() {
        read += page.items.len();
    }
    assert_eq!(read, 5, "a cap exactly on the last page is exhaustion");
    server.shutdown().await;
}

/// A failed page ends the traversal with the SDK's typed error and the failed cursor; the
/// objects of the pages before it are kept by `collect_complete`. A cursor of another list is
/// refused before anything is sent, and a traversal that failed never reads as ended.
#[tokio::test]
async fn a_failed_second_page_keeps_the_first_pages_items() {
    let server = TestServer::start(|request: &Recorded| {
        if request
            .query_all("starting_after")
            .first()
            .map(String::as_str)
            == Some("cus_d")
        {
            Reply::json(
                429,
                r#"{"error":{"type":"invalid_request_error","code":"rate_limit"}}"#,
            )
            .header("request-id", "req_page2")
            .header("stripe-rate-limited-reason", "global-rate")
        } else {
            respond(request)
        }
    })
    .await;
    let stripe = context(&server);
    let incomplete = customers(&stripe)
        .items()
        .unwrap()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(ids(&incomplete.collected), ["cus_e", "cus_d"]);
    let PagesError::Provider { error, failed, .. } = incomplete.error else {
        panic!("a failed page is `Provider`: {:?}", incomplete.error)
    };
    assert!(error.is_rate_limited());
    assert_eq!(
        error.meta().unwrap().request_id.as_deref(),
        Some("req_page2")
    );
    let Resume::At(cursor) = failed else {
        panic!("the failed page is the second one")
    };
    assert_eq!(cursor.id().as_str(), "cus_d");
    assert_eq!(server.requests().len(), 2, "nothing is retried");

    let other =
        CursorList::forward(customers(&stripe).set_email("x@example.test".to_owned())).unwrap();
    let mut pages = other.pages().starting_at(Resume::At(cursor));
    let Err(PagesError::Provider { error, .. }) = pages.next_page().await else {
        panic!("a cursor of another list is refused")
    };
    assert_eq!(error.protocol(), Some(ProtocolError::CursorBindingMismatch));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(
        server.requests().len(),
        2,
        "the refused cursor sent nothing"
    );
    server.shutdown().await;
}
