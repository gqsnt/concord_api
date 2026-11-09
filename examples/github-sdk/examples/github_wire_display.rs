//! The caller still uses the same typed inputs and prepares the same request bytes.

use concord::reqwest;
use concord_example_github_sdk::ids::{IssueNumber, Owner, RepoName, WorkflowRef};
use concord_example_github_sdk::params::{Direction, ETag, PerPage, PullSort};
use concord_example_github_sdk::{GitHub, GitHubConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let github = GitHub::new(
        reqwest::Client::new(),
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token: "synthetic-example-token".into(),
        },
    );
    let repo = github.repo(Owner::new("octo org"), RepoName::new("a/b"));
    let pulls = repo
        .pulls()
        .set_sort(PullSort::LongRunning)
        .set_direction(Direction::Asc)
        .set_per_page(PerPage::new(50)?)
        .prepare()?;
    assert_eq!(
        pulls.request().url().as_str(),
        "https://api.github.com/repos/octo%20org/a%2Fb/pulls?sort=long-running&direction=asc&per_page=50",
    );
    let issue = repo
        .issue(IssueNumber(1347))
        .set_if_none_match(ETag::new("W/\"previous\""))
        .prepare()?;
    assert_eq!(issue.request().headers()["if-none-match"], "W/\"previous\"");
    let runs = repo
        .actions()
        .workflow_runs(WorkflowRef::File("release/ci.yml".into()))
        .prepare()?;
    assert_eq!(
        runs.request().url().path(),
        "/repos/octo%20org/a%2Fb/actions/workflows/release%2Fci.yml/runs",
    );
    Ok(())
}
