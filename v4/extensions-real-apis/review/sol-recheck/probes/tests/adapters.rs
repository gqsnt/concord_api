use concord::reqwest::Client;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Resume, Outcome, sequential};
use std::num::{NonZeroUsize, NonZeroU32};

fn budget() -> Budget { Budget::new(NonZeroUsize::new(50).unwrap()) }

#[tokio::test]
async fn kubernetes_template_continue_is_sent_but_not_compared_on_first_fetch() {
    use concord_example_kubernetes_sdk::{KubeContext, ContinueToken};
    use concord_example_kubernetes_sdk::list::{ListPager, ListError};
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.query_all("continue"), ["seed-A"]);
        Reply::json(200, r#"{"metadata":{"resourceVersion":"100","continue":"seed-A"},"items":[{"metadata":{"name":"same-pod"}}]}"#)
    }).await;
    let kube = KubeContext::new(Client::builder().no_proxy().build().unwrap()).with_base_url(server.url()).unwrap();
    let template = kube.core().pods().set_continue_token(ContinueToken::from_server("seed-A"));
    let pages = ListPager::new(template, NonZeroU32::new(2).unwrap());
    let first = pages.fetch(None).await.unwrap();
    assert_eq!(first.items.len(), 1, "non-advancing continued page was exposed");
    assert!(matches!(pages.fetch(first.next).await, Err(ListError::ContinueDidNotAdvance)));
    assert_eq!(server.requests().len(), 2);
    println!("Kubernetes template continue=seed-A: first request returns same seed-A but succeeds with 1 item; only the second identical request fails");
    server.shutdown().await;
}

#[tokio::test]
async fn kubernetes_checks_current_token_and_keeps_snapshot_check() {
    use concord_example_kubernetes_sdk::{KubeContext, ResourceVersion};
    use concord_example_kubernetes_sdk::list::{ListPager, ListError, into_page};
    use concord_example_kubernetes_sdk::model::PodList;
    let server = TestServer::start(|r: &Recorded| {
        let next = match r.query_all("continue").first().map(String::as_str) {
            None => "token-A", Some("token-A") => "token-B", Some("token-B") => "token-B", _ => panic!("unexpected token")
        };
        Reply::json(200, format!(r#"{{"metadata":{{"resourceVersion":"100","continue":"{next}"}},"items":[]}}"#))
    }).await;
    let kube = KubeContext::new(Client::builder().no_proxy().build().unwrap()).with_base_url(server.url()).unwrap();
    let pages = ListPager::new(kube.core().pods(), NonZeroU32::new(2).unwrap());
    let mut chain = sequential(Resume::Start, budget(), |k| pages.fetch(k));
    assert!(chain.next().await.unwrap().is_ok());
    let second = chain.next().await.unwrap().unwrap();
    let key = second.next.unwrap();
    assert!(matches!(chain.next().await.unwrap(), Err(ListError::ContinueDidNotAdvance)));
    assert_eq!(chain.finish().outcome, Outcome::Failed);
    assert_eq!(server.requests().len(), 3);
    let changed: PodList = serde_json::from_str(r#"{"metadata":{"resourceVersion":"101","continue":"new"},"items":[]}"#).unwrap();
    assert!(matches!(into_page(changed, Some(&key)), Err(ListError::InconsistentSnapshot)));
    assert_eq!(key.snapshot(), &ResourceVersion::new("100"));
    println!("Kubernetes Start -> A -> B -> B: error at request 3; snapshot mismatch still refused");
    server.shutdown().await;
}

#[tokio::test]
async fn salesforce_all_three_adapters_compare_current_locator() {
    use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
    use concord_example_salesforce_sdk::bulk_query::{BulkQueryJob, ResultSets};
    use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl, QueryJobId, SObjectName};
    use concord_example_salesforce_sdk::{OrgContext, ProtocolErrorKind, SfError};
    use concord_example_salesforce_sdk::model::Account;
    use concord_example_salesforce_sdk::query::QueryPages;
    use concord_example_salesforce_sdk::soql::{FieldPath, Soql};
    const JOB: &str = "750A00000000001AAA";
    let server = TestServer::start(|r: &Recorded| {
        if r.path().ends_with("/results") {
            let next = if r.query_all("locator").is_empty() { "QUFB" } else { "QkJC" };
            Reply::text(200, "text/csv", "Id\n001A00000000001AAA\n")
                .header("sforce-locator", next).header("sforce-numberofrecords", "1")
        } else if r.path().ends_with("/resultPages") {
            let next = if r.query_all("locator").is_empty() { "QUFB" } else { "QkJC" };
            Reply::json(200, format!(r#"{{"resultPages":[],"done":false,"nextRecordsUrl":"/services/data/v68.0/jobs/query/{JOB}/resultPages?locator={next}"}}"#))
        } else {
            let next = if r.path().ends_with("/query") { "01gA0000000AbCdEAA-2" } else { "01gA0000000AbCdEAA-3" };
            Reply::json(200, format!(r#"{{"totalSize":9,"done":false,"nextRecordsUrl":"/services/data/v68.0/query/{next}","records":[]}}"#))
        }
    }).await;
    let http = Client::builder().no_proxy().resolve("sol.my.salesforce.test", server.addr()).build().unwrap();
    let ctx = OrgContext::new(http, InstanceUrl::parse(&format!("http://sol.my.salesforce.test:{}", server.addr().port())).unwrap(), ApiVersion::PINNED, AccessToken::new("synthetic-token").unwrap()).unwrap();
    let soql = Soql::select(SObjectName::new("Account").unwrap(), [FieldPath::parse("Id").unwrap()]).build().unwrap();
    let pages = QueryPages::<Account>::new(&ctx, soql);
    let mut chain = sequential(Resume::Start, budget(), |k| pages.fetch(k));
    assert!(chain.next().await.unwrap().is_ok());
    assert!(chain.next().await.unwrap().is_ok());
    assert!(matches!(chain.next().await.unwrap(), Err(SfError::Protocol(e)) if e.kind() == &ProtocolErrorKind::LocatorDidNotAdvance));
    assert_eq!(chain.finish().outcome, Outcome::Failed);
    #[derive(serde::Deserialize)] struct Row { #[serde(rename="Id")] _id: String }
    let job = BulkQueryJob::attach(&ctx, QueryJobId::new(JOB).unwrap(), BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf));
    let sets = ResultSets::<Row>::new(&job, &[FieldPath::parse("Id").unwrap()], None).unwrap();
    let mut chain = sequential(Resume::Start, budget(), |k| sets.fetch(k));
    assert!(chain.next().await.unwrap().is_ok());
    assert!(chain.next().await.unwrap().is_ok());
    assert!(matches!(chain.next().await.unwrap(), Err(SfError::Protocol(e)) if e.kind() == &ProtocolErrorKind::LocatorDidNotAdvance));
    assert_eq!(chain.finish().outcome, Outcome::Failed);
    let manifests = job.result_manifests();
    let k1 = manifests.fetch(None).await.unwrap().next.unwrap();
    let k2 = manifests.fetch(Some(k1)).await.unwrap().next.unwrap();
    assert!(matches!(manifests.fetch(Some(k2)).await, Err(SfError::Protocol(e)) if e.kind() == &ProtocolErrorKind::LocatorDidNotAdvance));
    assert_eq!(server.requests().len(), 9);
    println!("Salesforce query/results/manifests: all Start -> A -> B -> B, each fails at request 3");
    server.shutdown().await;
}

#[tokio::test]
async fn stripe_both_directions_check_current_id() {
    use concord_example_stripe_sdk::{StripeContext, SecretKey};
    use concord_example_stripe_sdk::pages::{CursorList, Direction};
    for direction in [Direction::Forward, Direction::Backward] {
        let server = TestServer::start(|r: &Recorded| {
            let key = r.query_all("starting_after").first().cloned().or_else(|| r.query_all("ending_before").first().cloned());
            let next = if key.is_none() { "cus_A" } else { "cus_B" };
            Reply::json(200, format!(r#"{{"object":"list","url":"/v1/customers","has_more":true,"data":[{{"id":"{next}","object":"customer","email":null,"name":null,"description":null,"created":1767225600}}]}}"#))
        }).await;
        let ctx = StripeContext::new(Client::builder().no_proxy().build().unwrap(), &SecretKey::new("sk_test_synthetic").unwrap()).with_base_url(format!("{}/v1", server.url())).unwrap();
        let list = CursorList::new(ctx.api().customers().list(), direction).unwrap();
        let mut chain = sequential(Resume::Start, budget(), |k| list.fetch(k));
        assert!(chain.next().await.unwrap().is_ok());
        assert!(chain.next().await.unwrap().is_ok());
        let err = chain.next().await.unwrap().unwrap_err();
        assert!(format!("{err:?}").contains("CursorDidNotAdvance"), "{err:?}");
        assert_eq!(chain.finish().outcome, Outcome::Failed);
        assert_eq!(server.requests().len(), 3);
        println!("Stripe {direction:?}: Start -> A -> B -> B, error at request 3");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn shopify_connection_checks_current_cursor() {
    use concord_example_shopify_sdk::{connection_page, Cursor, GraphqlResponse, PageError};
    use concord_example_shopify_sdk::connection::{Connection, Protocol};
    #[derive(serde::Deserialize)] struct Data { list: Connection<u64> }
    let mut key = None;
    for (n, next) in ["A", "B", "B"].into_iter().enumerate() {
        let response: GraphqlResponse<Data> = serde_json::from_value(serde_json::json!({"data":{"list":{"nodes":[n],"pageInfo":{"hasNextPage":true,"endCursor":next}}}})).unwrap();
        let p = connection_page(async { Ok(response) }, key.as_ref(), |d| Some(d.list)).await;
        if n < 2 { key = p.unwrap().next; }
        else { assert!(matches!(p, Err(PageError::Protocol { violation: Protocol::CursorDidNotAdvance, .. }))); }
    }
    assert_eq!(key.as_ref().map(Cursor::expose), Some("B"));
    println!("Shopify connection_page: Start -> A -> B -> B, error at page 3");
}
