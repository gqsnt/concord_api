//! Extension: list repository issue events, using the SDK's documented native context.
use std::num::NonZeroUsize;
use concord::reqwest::{Method, Url};
use concord_pagination::{Budget, Page, Resume, sequential};
use github::{RepoContext, meta::ResponseMeta};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct IssueEvent { pub id: u64, pub event: String }

/// The first request is configured by the caller; subsequent URLs come from GitHub.
pub async fn list_issue_events(repo: &RepoContext, per_page: u8) -> Result<Vec<IssueEvent>, Box<dyn std::error::Error>> {
    let mut first = repo.request(Method::GET, &["issues", "events"])?;
    first.url_mut().query_pairs_mut().append_pair("per_page", &per_page.to_string());
    let context = repo.context().clone();
    let mut pages = sequential(Resume::Start, Budget::new(NonZeroUsize::new(10).unwrap()), move |next: Option<Url>| {
        let context = context.clone();
        let mut request = first.try_clone().expect("bodyless request");
        async move {
            if let Some(url) = next {
                if !context.is_api_url(&url) { return Err("foreign pagination URL".into()); }
                *request.url_mut() = url;
            }
            let decoder = concord::Decoder::<Vec<IssueEvent>>::new::<concord::Json>();
            let response = match context.http().execute(request).await {
                Ok(response) => response,
                Err(error) => return decoder.finish(Err(error)).await
                    .map(|items| Page::new(items, None))
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>),
            };
            if !response.status().is_success() {
                return decoder.decode(response).await
                    .map(|items| Page::new(items, None))
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
            }
            let meta = ResponseMeta::capture(&response);
            let next = meta.links()?.next().cloned();
            if next.as_ref().is_some_and(|url| !context.is_api_url(url)) { return Err("foreign pagination URL".into()); }
            let items = decoder.decode(response).await?;
            Ok::<_, Box<dyn std::error::Error>>(Page::new(items, next))
        }
    });
    let mut items = Vec::new();
    while let Some(page) = pages.next().await { items.extend(page?.items); }
    if pages.finish().resume.is_some() { return Err("page budget exhausted".into()); }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use github::{GitHubConfig, GitHubContext, RepoRef};
    use server::{Reply, TestServer};

    #[tokio::test]
    async fn extension_traverses_link_and_inherits_context() {
        let server = TestServer::start(|r| {
            if r.query() == Some("per_page=1") {
                Reply::json(200, r#"[{"id":1,"event":"closed"}]"#).header("link", "<?page=2>; rel=\"next\"")
            } else {
                Reply::json(200, r#"[{"id":2,"event":"reopened"}]"#)
            }
        }).await;
        let context = GitHubContext::new(Default::default(), GitHubConfig { token: "test-token".into() }).with_base_url(server.url()).unwrap().set_user_agent("review-app");
        let repo = context.repo(RepoRef::new("org", "repo"));
        let events = list_issue_events(&repo, 1).await.unwrap();
        assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), [1, 2]);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].target, "/repos/org/repo/issues/events?per_page=1");
        assert_eq!(requests[1].target, "/repos/org/repo/issues/events?page=2");
        for request in requests {
            assert_eq!(request.header_all("authorization"), ["Bearer test-token"]);
            assert_eq!(request.header_all("user-agent"), ["review-app"]);
            assert_eq!(request.header_all("x-github-api-version"), ["2026-03-10"]);
        }
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}
