//! One call, one automatic Concord event. No request value is passed to a logging macro.

use concord::reqwest;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef, redirect_policy};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::INFO)
        .without_time()
        .with_ansi(false)
        .init();
    let token = std::env::var("GITHUB_TOKEN")?;
    let http = reqwest::Client::builder()
        .redirect(redirect_policy())
        .build()?;
    let github = GitHubContext::new(
        http,
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token,
        },
    );
    let repo = github.repo(RepoRef::new("octo-org", "octo-repo"));
    let _issues = repo.handle().issues().send().await?;
    Ok(())
}
