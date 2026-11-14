use concord::Body;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and every open bug of a repository, all pages.
pub async fn bugs() -> Result<Github, BoxError> {
    let quota = QuotaLog::default();
    let github = Github::new("my-app/1.0", quota.clone()).token(std::env::var("GITHUB_TOKEN")?);
    let mut issues = github.repo("rust-lang", "rust").issues().list()
        .state(IssueState::Open).labels(["C-bug"]).per_page(100)
        .pages().items();
    while let Some(issue) = issues.try_next().await? {
        println!("#{} {}", issue.number, issue.title);
    }
    Ok(github)
}

/// The repository again, only if it changed since its ETag. `None`: it did not.
pub async fn refresh(github: &Github, etag: &str) -> github::Result<Option<Repository>> {
    let answer = github.repo("rust-lang", "rust").get().send_if_none_match(etag).await?;
    Ok(answer.into_value())
}

/// Read the diff of a pull request, then merge exactly the commit that was read.
pub async fn merge(github: &Github, reviewed_sha: String) -> github::Result<()> {
    let pull = github.repo("octocat", "hello-world").pulls().pull(42);
    let diff = pull.diff().await?;
    println!("{} bytes of diff", diff.len());
    let merged = pull.merge(MergePull {
        sha: Some(reviewed_sha),
        merge_method: Some("squash".into()),
        ..Default::default()
    }).await?;
    println!("{}", merged.message);
    Ok(())
}

/// A file of a release, sent to the upload host.
pub async fn upload(github: &Github, zip: Vec<u8>) -> github::Result<()> {
    let releases = github.repo("octocat", "hello-world").releases();
    let release = releases.by_tag("v1.0.0").await?;
    let size = zip.len() as u64;
    let asset = releases.release(release.id)
        .asset("app.zip", Body::new(zip, "application/zip"), size)
        .label("Linux build")
        .await?;
    println!("{}", asset.browser_download_url);
    Ok(())
}

/// A search, page by page; then what is left of the search quota.
pub async fn search(github: &Github, quota: &QuotaLog) -> github::Result<()> {
    let mut pages = github.search().issues("repo:rust-lang/rust is:open label:C-bug")
        .per_page(100).pages();
    while let Some(page) = pages.try_next().await? {
        if page.incomplete_results {
            eprintln!("GitHub did not look everywhere");
        }
        for issue in page.items {
            println!("{}", issue.title);
        }
    }
    let log = quota.lock().unwrap_or_else(|e| e.into_inner());
    let left = log.get("search").and_then(|h| h.get("x-ratelimit-remaining"));
    println!("search quota left: {left:?}");
    Ok(())
}
