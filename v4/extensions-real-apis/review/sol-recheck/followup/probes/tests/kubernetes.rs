use concord::reqwest::Client;
use concord_example_kubernetes_sdk::{ContinueToken, KubeContext, ResourceVersion, ResourceVersionMatch};
use concord_example_kubernetes_sdk::list::{Continuation, ListError, ListPager};
use concord_example_testserver::{Recorded, Reply, TestServer};
use std::num::NonZeroU32;

#[tokio::test]
async fn template_token_is_cleared_and_current_key_is_checked() {
    let server = TestServer::start(|r: &Recorded| {
        let key = r.query_all("continue");
        let next = match key.first().map(String::as_str) {
            None => "seed-A", Some("seed-A") => "next-B", Some("next-B") => "next-B", _ => panic!("unexpected token {key:?}")
        };
        Reply::json(200, format!(r#"{{"metadata":{{"resourceVersion":"100","continue":"{next}"}},"items":[{{"metadata":{{"name":"pod"}}}}]}}"#))
    }).await;
    let kube = KubeContext::new(Client::builder().no_proxy().build().unwrap()).with_base_url(server.url()).unwrap();
    let template = kube.core().pods()
        .set_continue_token(ContinueToken::from_server("seed-A"))
        .set_resource_version(ResourceVersion::new("77"))
        .set_resource_version_match(ResourceVersionMatch::NotOlderThan);
    let pages = ListPager::new(template, NonZeroU32::new(2).unwrap());
    let a = pages.fetch(None).await.unwrap();
    assert_eq!(a.items.len(),1,"first reply may return the ignored template's token");
    let b = pages.fetch(a.next).await.unwrap();
    assert!(matches!(pages.fetch(b.next).await, Err(ListError::ContinueDidNotAdvance)));
    let requests = server.requests();
    let tokens: Vec<_> = requests.iter().map(|r| r.query_all("continue")).collect();
    assert_eq!(tokens,[vec![], vec!["seed-A".to_string()], vec!["next-B".to_string()]]);
    assert_eq!(requests[0].query_all("resourceVersion"),["77"]);
    assert_eq!(requests[0].query_all("resourceVersionMatch"),["NotOlderThan"]);
    for r in &requests[1..] {
        assert!(r.query_all("resourceVersion").is_empty());
        assert!(r.query_all("resourceVersionMatch").is_empty());
        assert_eq!(r.query_all("limit"),["2"]);
    }
    // Calling the factory from Start again must not retain a previous key.
    pages.fetch(None).await.unwrap();
    assert!(server.requests()[3].query_all("continue").is_empty());
    println!("SOL-R2: sent [None, seed-A, next-B, None]; repeat rejected at request 3; first RV preserved and continued RV cleared");
    server.shutdown().await;
}

#[tokio::test]
async fn explicit_resume_overrides_template_and_checks_snapshot() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.query_all("continue"),["key-C"]);
        assert!(r.query_all("resourceVersion").is_empty());
        Reply::json(200, r#"{"metadata":{"resourceVersion":"101","continue":"key-D"},"items":[]}"#)
    }).await;
    let kube = KubeContext::new(Client::builder().no_proxy().build().unwrap()).with_base_url(server.url()).unwrap();
    let pages = ListPager::new(kube.core().pods().set_continue_token(ContinueToken::from_server("template-unused")).set_resource_version(ResourceVersion::new("77")), NonZeroU32::new(2).unwrap());
    let key = Continuation::resume(ContinueToken::from_server("key-C"),ResourceVersion::new("100"));
    assert!(matches!(pages.fetch(Some(key)).await,Err(ListError::InconsistentSnapshot)));
    assert_eq!(server.requests().len(),1);
    println!("explicit resume sends key-C, never template-unused; inconsistent snapshot still refused");
    server.shutdown().await;
}
