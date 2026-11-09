//! GH-A against api.github.com: a small, read-only activity audit of one repository.
//!
//! Needs `GITHUB_TOKEN` and `GH_REPO` (`owner/name`); without them it prints `NOT_RUN` and exits
//! 0. It sends only `GET` requests, serially, within small page budgets (one issue page of ten,
//! one page per child collection). It prints counts, the completion and rate-limit numbers, never
//! titles, bodies or the token.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord_example_github_app::audit::{AuditBudget, AuditInput, activity_audit};
use concord_example_github_app::requests::last_rate;
use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::params::{PerPage, StateFilter};
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef, redirect_policy};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (Ok(token), Ok(repo)) = (std::env::var("GITHUB_TOKEN"), std::env::var("GH_REPO")) else {
        println!("NOT_RUN: GH-A live smoke needs GITHUB_TOKEN and GH_REPO=owner/name");
        return Ok(());
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return Err("GH_REPO must be owner/name".into());
    };
    let http = reqwest::Client::builder()
        .redirect(redirect_policy())
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let github = GitHubContext::new(
        http,
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token,
        },
    );
    let repo = github.repo(RepoRef::new(owner, name));
    let one = NonZeroUsize::MIN;
    let report = activity_audit(
        &repo,
        AuditInput {
            filter: IssueFilter {
                state: Some(StateFilter::All),
                per_page: Some(PerPage::new(10)?),
                ..IssueFilter::default()
            },
            budget: AuditBudget {
                issue_pages: one,
                child_pages: one,
            },
            repository_etag: None,
            from: None,
        },
    )
    .await;
    println!(
        "GH-A live: {} issues, {} pull requests, {} requests, completion {:?}",
        report.issues().count(),
        report.pull_requests().count(),
        report.requests.len(),
        report.completion,
    );
    if let Some(rate) = last_rate(&report.requests) {
        println!(
            "rate limit: remaining {:?} of {:?}",
            rate.remaining, rate.limit
        );
    }
    Ok(())
}
