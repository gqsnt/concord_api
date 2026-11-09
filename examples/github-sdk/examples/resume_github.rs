//! Start at the first page or at a stored continuation supplied as the first argument.
use std::num::NonZeroUsize;

use concord::reqwest;
use concord_example_github_sdk::ids::IssueNumber;
use concord_example_github_sdk::params::PerPage;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef};
use concord_pagination::Resume;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()?;
    let github = GitHubContext::new(
        http,
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token: std::env::var("GITHUB_TOKEN")?,
        },
    );
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let per_page = PerPage::new(50)?;
    let comments = repo.issue_comment_pages(IssueNumber(7), |op| op.set_per_page(per_page));
    let from = match std::env::args().nth(1) {
        Some(stored) => Resume::At(comments.restore(&stored)?),
        None => Resume::Start,
    };
    let mut pages = comments
        .pages()
        .starting_at(from)
        .refuse_repeated_keys()
        .max_pages(NonZeroUsize::new(20).unwrap());
    while let Some(page) = pages.next_page().await? {
        for comment in page.items {
            println!("{}", comment.id);
        }
        // Persist page.next.as_ref().map(|link| link.to_stored()) in application storage.
        // That text contains the raw URL; keep it out of ordinary logs.
    }
    Ok(())
}
