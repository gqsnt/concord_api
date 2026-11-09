//! The two connected workflows end to end against fixtures, with the required failure: an HTTP
//! success whose envelope says `success: false` surfaces Cloudflare's errors, never an empty
//! result.

mod support;

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_cloudflare_zones::concord_pagination::{Outcome, PagesError, Resume};
use concord_sdk_cloudflare_zones::{
    AccountId, EnvelopeProblem, Error, RecordId, RecordType, RecordsPerPage, SendPolicy, ZoneId,
    ZoneStatus, ZonesPerPage,
};
use support::{ACCOUNT, RECORD, ZONE, client, client_with, ok, unexpected};

fn zones_page(r: &Recorded) -> Option<&'static str> {
    let filter = format!("/client/v4/zones?account.id={ACCOUNT}");
    match r.target.strip_prefix(&filter)? {
        "&page=1&per_page=5" => Some("zones_page1.json"),
        "&page=2&per_page=5" => Some("zones_page2.json"),
        _ => None,
    }
}

/// The fixture service of both workflows: List Zones (2 pages), Zone Details, List DNS Records
/// (2 pages), DNS Record Details.
fn service(r: &Recorded) -> Reply {
    if let Some(name) = zones_page(r) {
        return ok(name);
    }
    let zone = format!("/client/v4/zones/{ZONE}");
    let records = format!("{zone}/dns_records");
    match r.target.as_str() {
        t if t == zone => ok("zone_details.json"),
        t if t == format!("{records}?page=1&per_page=2") => ok("dns_records_page1.json"),
        t if t == format!("{records}?page=2&per_page=2") => ok("dns_records_page2.json"),
        t if t == format!("{records}/{RECORD}") => ok("dns_record_details.json"),
        t => unexpected(t),
    }
}

fn five() -> ZonesPerPage {
    ZonesPerPage::new(5).unwrap()
}

#[tokio::test]
async fn workflow_1_zones_of_an_account_then_one_zone() {
    let server = TestServer::start(service).await;
    let cf = client(&server);

    let zones = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(five())
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let names: Vec<_> = zones.iter().map(|z| z.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "alpha.example.com",
            "beta.example.com",
            "gamma.example.com",
            "delta.example.com",
            "epsilon.example.com",
            "zeta.example.com",
        ]
    );
    assert!(
        zones
            .iter()
            .all(|z| z.account.id.as_ref().unwrap().as_str() == ACCOUNT)
    );
    assert_eq!(zones[1].status, Some(ZoneStatus::Pending));
    assert_eq!(zones[1].activated_on, None);
    assert_eq!(zones[1].original_name_servers, None);

    // Fetch the zone found on the second page by its identifier.
    let selected = zones.last().unwrap();
    let response = cf.zone(selected.id.clone()).get().send().await.unwrap();
    let zone = response.result;
    assert_eq!(zone.id, selected.id);
    assert_eq!(zone.name, "zeta.example.com");
    assert_eq!(zone.status, Some(ZoneStatus::Active));
    assert_eq!(zone.name_servers.len(), 2);

    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            format!("/client/v4/zones?account.id={ACCOUNT}&page=1&per_page=5"),
            format!("/client/v4/zones?account.id={ACCOUNT}&page=2&per_page=5"),
            format!("/client/v4/zones/{ZONE}"),
        ]
    );
}

#[tokio::test]
async fn workflow_1_page_by_page_with_metadata() {
    let server = TestServer::start(service).await;
    let cf = client(&server);
    let mut pages = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(five())
        .pages();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 5);
    assert_eq!(first.next, NonZeroU32::new(2));
    assert_eq!((first.meta.page.get(), first.meta.total_pages), (1, 2));
    assert_eq!(first.meta.result_info.total_count, Some(6));
    assert_eq!(first.meta.meta.status(), 200);
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.next, None);
    // `None` only after the provider's last page.
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
}

#[tokio::test]
async fn workflow_2_dns_records_of_the_zone_then_one_record() {
    let server = TestServer::start(service).await;
    let cf = client(&server);
    let zone = cf.zone(ZoneId::from(ZONE));

    let records = zone
        .dns_records()
        .list()
        .set_per_page(RecordsPerPage::new(2).unwrap())
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let kinds: Vec<_> = records.iter().map(|r| r.record_type.clone()).collect();
    assert_eq!(kinds, [RecordType::A, RecordType::Aaaa, RecordType::Txt]);
    assert_eq!(records[0].content.as_deref(), Some("192.0.2.10"));
    assert_eq!(records[0].tags, ["owner:dns-team"]);
    assert!(records[1].ttl_is_automatic());
    assert_eq!(records[2].comment, None);

    let selected = records
        .iter()
        .find(|r| r.record_type == RecordType::Aaaa)
        .unwrap();
    let record = zone
        .dns_record(selected.id.clone())
        .get()
        .send()
        .await
        .unwrap()
        .result;
    assert_eq!(record.id.as_str(), RECORD);
    assert_eq!(record.name, "www.zeta.example.com");
    assert_eq!(record.content.as_deref(), Some("2001:db8::10"));
    assert_eq!(record.comment.as_deref(), Some("synthetic www record"));
    assert_eq!(record.proxiable, Some(true));

    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            format!("/client/v4/zones/{ZONE}/dns_records?page=1&per_page=2"),
            format!("/client/v4/zones/{ZONE}/dns_records?page=2&per_page=2"),
            format!("/client/v4/zones/{ZONE}/dns_records/{RECORD}"),
        ]
    );
}

#[tokio::test]
async fn both_workflows_chained_from_the_account() {
    let server = TestServer::start(service).await;
    let cf = client(&server);
    let zones = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(five())
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let zone = cf
        .zone(zones[5].id.clone())
        .get()
        .send()
        .await
        .unwrap()
        .result;
    let records = cf
        .zone(zone.id.clone())
        .dns_records()
        .list()
        .set_per_page(RecordsPerPage::new(2).unwrap())
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let record = cf
        .zone(zone.id)
        .dns_record(records[1].id.clone())
        .get()
        .send()
        .await
        .unwrap()
        .result;
    assert_eq!(record.name, "www.zeta.example.com");
    assert_eq!(server.requests().len(), 6);
}

// ---------------------------------------------------------------------------------------------
// The required failure: HTTP 200 with `success: false`

#[tokio::test]
async fn success_false_mid_listing_surfaces_errors_and_keeps_the_prefix() {
    let failing = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&failing);
    let server = TestServer::start(move |r| match zones_page(r) {
        Some("zones_page2.json") if flag.load(Ordering::SeqCst) => ok("failure_success_false.json"),
        _ => service(r),
    })
    .await;
    let cf = client(&server);
    let list = cf
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(five());

    let incomplete = list
        .clone()
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 5, "the first page is kept");
    let (error, failed, _) = incomplete.error.into_provider_error().unwrap();
    assert_eq!(failed, Resume::At(NonZeroU32::new(2).unwrap()));
    let Error::Provider(provider) = error else {
        panic!("expected a provider error, got {error:?}");
    };
    assert_eq!(provider.status(), 200);
    assert_eq!(provider.document().unwrap().codes(), [1001, 1002]);
    assert_eq!(
        provider.errors()[0].message,
        "synthetic refusal: the identifier is not valid"
    );

    // The caller resumes from the failed page once Cloudflare answers again.
    failing.store(false, Ordering::SeqCst);
    let rest = list
        .pages()
        .starting_at(failed)
        .items()
        .collect_complete()
        .await
        .unwrap();
    let names: Vec<_> = rest.iter().map(|z| z.name.as_str()).collect();
    assert_eq!(names, ["zeta.example.com"]);
}

#[tokio::test]
async fn success_false_on_a_first_page_is_never_an_empty_listing() {
    let server = TestServer::start(|_| ok("failure_success_false.json")).await;
    let cf = client(&server);
    let error = cf
        .zone(ZoneId::from(ZONE))
        .dns_records()
        .list()
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(error.collected.is_empty());
    let provider = error
        .error
        .provider_error()
        .and_then(Error::provider)
        .unwrap();
    assert_eq!(provider.status(), 200);
    assert_eq!(provider.document().unwrap().codes(), [1001, 1002]);
}

#[tokio::test]
async fn success_false_on_details_is_an_error_not_an_empty_object() {
    let server = TestServer::start(|_| ok("failure_success_false.json")).await;
    let cf = client(&server);
    let zone = cf.zone(ZoneId::from(ZONE)).get().send().await.unwrap_err();
    let record = cf
        .zone(ZoneId::from(ZONE))
        .dns_record(RecordId::from(RECORD))
        .get()
        .send()
        .await
        .unwrap_err();
    for error in [zone, record] {
        let provider = error.provider().unwrap();
        assert_eq!(provider.status(), 200);
        assert_eq!(provider.document().unwrap().codes(), [1001, 1002]);
    }
}

// ---------------------------------------------------------------------------------------------
// Walk boundaries

#[tokio::test]
async fn an_empty_listing_is_complete() {
    let server = TestServer::start(|_| ok("empty_listing.json")).await;
    let zones = client(&server)
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(zones.is_empty());
    assert_eq!(
        server.single().target,
        format!("/client/v4/zones?account.id={ACCOUNT}&page=1")
    );
}

#[tokio::test]
async fn a_page_without_paging_info_or_from_elsewhere_stops_the_walk() {
    let server = TestServer::start(|r| match r.query() {
        Some("page=1") => ok("page_without_result_info.json"),
        _ => ok("page_mismatch.json"),
    })
    .await;
    let cf = client(&server);

    let error = cf.zones().list().pages().next_page().await.unwrap_err();
    let Some(Error::Envelope(envelope)) = error.provider_error() else {
        panic!("expected an envelope error");
    };
    assert_eq!(envelope.problem(), EnvelopeProblem::MissingPageInfo);

    let from = Resume::At(NonZeroU32::new(2).unwrap());
    let error = cf
        .zones()
        .list()
        .pages()
        .starting_at(from)
        .next_page()
        .await
        .unwrap_err();
    let Some(Error::Envelope(envelope)) = error.provider_error() else {
        panic!("expected an envelope error");
    };
    assert_eq!(
        envelope.problem(),
        EnvelopeProblem::PageMismatch {
            requested: NonZeroU32::new(2).unwrap(),
            returned: 3,
        }
    );
}

#[tokio::test]
async fn a_page_cap_is_a_bound_not_completeness() {
    let server = TestServer::start(service).await;
    let error = client(&server)
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(five())
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(error.collected.len(), 5);
    assert!(matches!(error.error, PagesError::Bounded { .. }));
    assert_eq!(
        error.error.resume(),
        Some(&Resume::At(NonZeroU32::new(2).unwrap()))
    );
}

#[tokio::test]
async fn a_rate_limited_page_is_retried_by_the_opt_in_policy() {
    let limited = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&limited);
    let server = TestServer::start(move |r| match zones_page(r) {
        Some("zones_page2.json") if flag.swap(false, Ordering::SeqCst) => {
            Reply::json(429, support::fixture("error_429.json"))
                .header("retry-after", "0")
                .header("ratelimit", r#""default";r=0;t=1"#)
        }
        _ => service(r),
    })
    .await;
    let policy = SendPolicy::default()
        .retry_rate_limited(NonZeroUsize::new(2).unwrap(), Duration::from_secs(1));
    let zones = client_with(&server, policy)
        .zones()
        .list()
        .set_account_id(AccountId::from(ACCOUNT))
        .set_per_page(five())
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(zones.len(), 6);
    assert_eq!(server.requests().len(), 3);
}
