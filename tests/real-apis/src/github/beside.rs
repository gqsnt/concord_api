use concord::http::{HeaderMap, HeaderName};
use concord::transport::{TransportError, TryInfo};
use concord::{reqwest, Middleware, Next};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// A page of a search. `incomplete_results`: GitHub stopped looking; more may match.
#[derive(Debug, Deserialize)]
pub struct SearchPage<T> {
    pub total_count: u64,
    pub incomplete_results: bool,
    pub items: Vec<T>,
}

impl<T: Send> concord::page::IntoItems for SearchPage<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// The last rate-limit headers of each resource (`core`, `search`, …), shared with the caller.
pub type QuotaLog = Arc<Mutex<BTreeMap<String, HeaderMap>>>;

/// `middleware ObserveQuota(quota.clone());`: it sees every response: each page, each attempt.
pub struct ObserveQuota(pub QuotaLog);

impl Middleware for ObserveQuota {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let answer = next.send(request, info).await?;
        let mut kept = HeaderMap::new();
        for name in [
            "x-ratelimit-limit", "x-ratelimit-remaining", "x-ratelimit-used",
            "x-ratelimit-reset", "x-ratelimit-resource", "retry-after",
        ] {
            if let Some(value) = answer.headers().get(name) {
                kept.insert(HeaderName::from_static(name), value.clone());
            }
        }
        let resource = answer.headers().get("x-ratelimit-resource")
            .and_then(|v| v.to_str().ok()).unwrap_or("unknown").to_owned();
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(resource, kept);
        Ok(answer)
    }
}

/// The query entry of `user.repos`: an error here is a Build error, and the call is not sent.
pub fn checked_repo_kind(
    kind: &Option<UserRepoType>, visibility: &Option<Visibility>, affiliation: &Option<Vec<String>>,
) -> Result<Option<UserRepoType>, &'static str> {
    if kind.is_some() && (visibility.is_some() || affiliation.is_some()) {
        return Err("`type` cannot go with `visibility` or `affiliation`");
    }
    Ok(kind.clone())
}

/// A repository that was renamed or moved: GitHub responds with `301` and its new location.
/// This follows at most four moves, on the same origin only, and keeps the generated call (its
/// reader, its middleware, its credentials).
pub async fn get_moved(github: &Github, owner: &str, name: &str)
    -> Result<Repository, Box<dyn std::error::Error + Send + Sync>>
{
    let mut url = github.repo(owner, name).get().request()?.url().clone();
    for _ in 0..4 {
        let target = url.clone();
        let result = github.repo(owner, name).get()
            .edit_request(move |request| *request.url_mut() = target.clone())
            .await;
        match result {
            Err(error) if error.is_status(301) => {
                let location = error.headers().and_then(|h| h.get("location"))
                    .ok_or("a 301 with no Location")?;
                let next = url.join(location)?;
                if next.origin() != url.origin() {
                    return Err("the repository moved to another origin".into());
                }
                url = next;
            }
            other => return Ok(other?),
        }
    }
    Err("more than four moves".into())
}
