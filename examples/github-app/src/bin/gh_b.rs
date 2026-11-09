//! GH-B against api.github.com: retrieves one workflow artifact into a file.
//!
//! Needs `GITHUB_TOKEN`, `GH_REPO` (`owner/name`), `GH_ARTIFACT` (the artifact name) and
//! `GH_ARTIFACT_OUT` (the output file); without them it prints `NOT_RUN` and exits 0. Read-only
//! `GET` requests, serial, small page budgets, at most 64 MiB written; the token is never sent to
//! the download host.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord_example_github_app::artifacts::{
    RetrievalBudget, RetrievalInput, RunSelector, retrieve_artifact,
};
use concord_example_github_app::sink::FileSink;
use concord_example_github_sdk::download::DownloadLimits;
use concord_example_github_sdk::filters::RunFilter;
use concord_example_github_sdk::models::Conclusion;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef, redirect_policy};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vars = ["GITHUB_TOKEN", "GH_REPO", "GH_ARTIFACT", "GH_ARTIFACT_OUT"].map(std::env::var);
    let [Ok(token), Ok(repo), Ok(artifact), Ok(out)] = vars else {
        println!(
            "NOT_RUN: GH-B live smoke needs GITHUB_TOKEN, GH_REPO=owner/name, GH_ARTIFACT and GH_ARTIFACT_OUT"
        );
        return Ok(());
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return Err("GH_REPO must be owner/name".into());
    };
    let http = reqwest::Client::builder()
        .redirect(redirect_policy())
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let github = GitHubContext::new(
        http,
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token,
        },
    );
    let repo = github.repo(RepoRef::new(owner, name));
    let limit = 64 * 1024 * 1024;
    let two = NonZeroUsize::new(2).expect("2 > 0");
    let input = RetrievalInput {
        runs: RunFilter::default(),
        select: RunSelector {
            conclusion: Some(Conclusion::Success),
        },
        artifact_name: artifact,
        budget: RetrievalBudget {
            run_pages: two,
            job_pages: two,
            artifact_pages: two,
        },
        limits: DownloadLimits { max_bytes: limit },
    };
    let mut sink = FileSink::new(out, limit);
    let retrieval = retrieve_artifact(&repo, &input, &mut sink).await;
    println!(
        "GH-B live: {} requests, outcome {:?}",
        retrieval.requests.len(),
        retrieval.outcome
    );
    Ok(())
}
