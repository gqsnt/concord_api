//! Probe (review): a provider whose `Link: rel="next"` repeats the URL that was just requested.
//! The GitHub adapter does not refuse it (Stripe and Shopify refuse a non-advancing cursor); only
//! the page budget ends the chain.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::reqwest;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef, redirect_policy};
use concord_example_github_sdk::filters::IssueFilter;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome, Resume, sequential};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../examples/github-app/tests/fixtures/synthetic/",
            $name
        ))
    };
}

#[tokio::test]
async fn github_link_next_equal_to_the_request_loops_until_the_budget() {
    let served = Arc::new(AtomicUsize::new(0));
    let counter = served.clone();
    let server = TestServer::start(move |r: &Recorded| {
        counter.fetch_add(1, Ordering::SeqCst);
        let o = format!("http://{}", r.host);
        // Every page, including page 2 itself, says: next is page 2.
        Reply::json(200, fixture!("issues-a-1.json")).header(
            "link",
            format!("<{o}/repositories/101/issues?page=2>; rel=\"next\""),
        )
    })
    .await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(redirect_policy())
        .resolve("api.github.test", server.addr())
        .build()
        .unwrap();
    let github = GitHubContext::new(http, GitHubConfig { token: "ghp_SYNTHETIC".into() })
        .with_base_url(format!("http://api.github.test:{}", server.addr().port()))
        .unwrap();
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let pages = repo.issue_pages(repo.issues(&IssueFilter::default()));
    let budget = Budget::new(NonZeroUsize::new(50).unwrap());
    let mut chain = sequential(Resume::Start, budget, |key| pages.fetch(key));
    let mut items = 0usize;
    let mut errors = 0usize;
    while let Some(page) = chain.next().await {
        match page {
            Ok(page) => items += page.items.len(),
            Err(_) => errors += 1,
        }
    }
    let report = chain.finish();
    println!(
        "requests={} items={} errors={} outcome={:?}",
        served.load(Ordering::SeqCst),
        items,
        errors,
        report.outcome
    );
    // Observed: 50 requests, the same 2 items 50 times, no error, `Bounded` with a resume point
    // that would loop again.
    assert_eq!(served.load(Ordering::SeqCst), 50);
    assert_eq!(items, 100);
    assert_eq!(errors, 0);
    assert_eq!(report.outcome, Outcome::Bounded);
    assert!(report.resume.is_some());
    server.shutdown().await;
}
