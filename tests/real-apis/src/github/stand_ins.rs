// The data types that the page names and does not print, cut to the fields that its code uses.

macro_rules! answers {
    ($($name:ident),*) => { $( #[derive(Debug, serde::Deserialize)] pub struct $name {} )* };
}
macro_rules! inputs {
    ($($name:ident),*) => { $( #[derive(Clone, Debug, serde::Serialize)] pub struct $name {} )* };
}
answers!(Comment, ContentEntry, Contents, ContentWrite, ContributorStats, GithubProblem, Pull,
    PullFile, RateLimits, Repository, User);
inputs!(NewIssue, NewPull, NewRelease, NewRepository, PutContent, Direction, IssueSort,
    OrgRepoType, PublicRepoType, PullSort, RepoSort, UserRepoType, Visibility);

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
    All,
}
#[derive(Debug, serde::Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct MergePull {
    pub commit_title: Option<String>,
    pub sha: Option<String>,
    pub merge_method: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct MergeResult {
    pub merged: bool,
    pub message: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct Release {
    pub id: u64,
}
#[derive(Debug, serde::Deserialize)]
pub struct Asset {
    pub browser_download_url: String,
}
