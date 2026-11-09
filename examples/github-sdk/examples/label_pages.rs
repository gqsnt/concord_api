//! List labels through the contextual policy, including every provider-issued next URL.
use concord_example_github_sdk::params::PerPage;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef, redirect_policy};
use std::num::NonZeroUsize;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = concord::reqwest::Client::builder()
        .redirect(redirect_policy())
        .build()?;
    let github = GitHubContext::new(
        http,
        GitHubConfig {
            token: std::env::var("GITHUB_TOKEN")?,
        },
    );
    let repo = github.repo(RepoRef::new("octocat", "Hello-World"));
    let listing = repo.label_pages(repo.handle().labels().set_per_page(PerPage::new(50)?));
    let mut pages = listing.pages().max_pages(NonZeroUsize::new(20).unwrap());
    // A cap or failure is an error carrying a resume position, never a successful end.
    while let Some(page) = pages.next_page().await? {
        for label in page.items {
            println!("{}", label.name);
        }
    }
    Ok(())
}
