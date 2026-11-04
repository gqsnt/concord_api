//! Domain values become each service's own representation, through adapters that return the
//! generated operations (D01), refuse what a service cannot represent before any I/O (D02), and
//! report each service independently (D03). Run against the fixture's fake services.

use concord::ErrorKind;
use concord::reqwest;
use concord_example_app::catalog_repr::{self, CatalogReprError};
use concord_example_app::domain::{AccountId, Moment, NanosOutOfRange};
use concord_example_app::ledger_repr::{self, LedgerReprError};
use concord_example_app::report::{Outcome, account_report};
use concord_example_fixture::{
    Reply, Server, catalog as catalog_service, rfc3339_seconds, services,
};
use concord_example_sdk::{
    Catalog, CatalogActivity, CatalogConfig, Ledger, LedgerConfig, LedgerEntries,
};

fn catalog(url: &str) -> Catalog {
    Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k-1".into() })
        .with_base_url(format!("{url}/catalog/v1"))
        .unwrap()
}

fn ledger(url: &str) -> Ledger {
    Ledger::new(
        reqwest::Client::new(),
        LedgerConfig {
            token: "Bearer t".into(),
        },
    )
    .with_base_url(format!("{url}/ledger/v1"))
    .unwrap()
}

const ACCOUNT: AccountId = AccountId::new(42);
/// 2026-01-15T10:00:00Z.
const JAN_15: i64 = 1_768_471_200;

fn moment(seconds: i64, nanos: u32) -> Moment {
    Moment::new(seconds, nanos).unwrap()
}

#[tokio::test]
async fn d01_one_account_reaches_each_service_in_its_own_representation() {
    let server = services().await;
    let (catalog, ledger) = (catalog(&server.url()), ledger(&server.url()));
    let since = Moment::from_unix_seconds(JAN_15);

    // The adapters return the generated operations themselves.
    let activity: CatalogActivity = catalog_repr::activity(&catalog, ACCOUNT, Some(since)).unwrap();
    let entries: LedgerEntries = ledger_repr::entries(&ledger, ACCOUNT, Some(since)).unwrap();
    assert_eq!(server.count(), 0, "converting sends nothing");

    let activity = activity.send().await.unwrap();
    // A caller setter on the returned operation still applies.
    let entries = entries.set_page_size(2).send().await.unwrap();

    let ids: Vec<_> = activity.events.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["event-001", "event-002", "event-003"]);
    let ids: Vec<_> = entries.data.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["entry-001", "entry-002"]);
    assert_eq!(entries.paging.next.as_deref(), Some("ledger-from-2"));

    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].path(), "/catalog/v1/accounts/42/activity");
    assert_eq!(seen[0].query(), Some("since=2026-01-15T10%3A00%3A00Z"));
    assert_eq!(seen[0].query_all("since"), ["2026-01-15T10:00:00Z"]);
    assert_eq!(seen[1].path(), "/ledger/v1/entries");
    assert_eq!(
        seen[1].query(),
        Some("account=acct-002a&from_ms=1768471200000&page_size=2")
    );
}

#[tokio::test]
async fn d01_no_lower_bound_and_caller_setters_on_the_operation() {
    let server = services().await;
    let (catalog, ledger) = (catalog(&server.url()), ledger(&server.url()));

    // Absent `since`: no `since` at all, not an empty one.
    catalog_repr::activity(&catalog, ACCOUNT, None)
        .unwrap()
        .send()
        .await
        .unwrap();
    // The caller may still replace what the adapter set, with the service's own text.
    ledger_repr::entries(&ledger, ACCOUNT, None)
        .unwrap()
        .set_from_ms("1769904000000")
        .set_page_token("ledger-from-1")
        .send()
        .await
        .unwrap();
    // `prepare()` is available too, without I/O.
    let prepared = ledger_repr::entries(&ledger, AccountId::new(7), None)
        .unwrap()
        .prepare()
        .unwrap();
    assert_eq!(
        prepared.request().url().query(),
        Some("account=acct-0007&page_size=10")
    );

    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].target, "/catalog/v1/accounts/42/activity");
    assert_eq!(
        seen[1].query(),
        Some("account=acct-002a&from_ms=1769904000000&page_size=10&page_token=ledger-from-1")
    );
}

#[test]
fn d01_representations_of_account_ids() {
    assert_eq!(catalog_repr::account(AccountId::new(0)), "0");
    assert_eq!(catalog_repr::account(AccountId::new(42)), "42");
    assert_eq!(
        catalog_repr::account(AccountId::new(u64::MAX)),
        "18446744073709551615"
    );
    assert_eq!(ledger_repr::account(AccountId::new(0)), "acct-0000");
    assert_eq!(ledger_repr::account(AccountId::new(42)), "acct-002a");
    assert_eq!(ledger_repr::account(AccountId::new(0x1_2345)), "acct-12345");
    assert_eq!(
        ledger_repr::account(AccountId::new(u64::MAX)),
        "acct-ffffffffffffffff"
    );
}

#[test]
fn d01_catalog_since_is_whole_second_rfc3339() {
    let since = |s| catalog_repr::since(Moment::from_unix_seconds(s)).unwrap();
    assert_eq!(since(JAN_15), "2026-01-15T10:00:00Z");
    assert_eq!(since(0), "1970-01-01T00:00:00Z");
    assert_eq!(since(-1), "1969-12-31T23:59:59Z");
    assert_eq!(since(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(since(-62_167_219_200), "0000-01-01T00:00:00Z");
    assert_eq!(since(253_402_300_799), "9999-12-31T23:59:59Z");

    // Every written text reads back as the same second (the fixture's independent parser), across
    // the whole four-digit range, including leap days and days before 1970.
    let mut seconds = -62_167_219_200_i64;
    while seconds <= 253_402_300_799 {
        let text = since(seconds);
        assert_eq!(rfc3339_seconds(&text), Some(seconds), "{text}");
        seconds += 86_400 * 37 + 3_671;
    }
}

#[test]
fn d01_ledger_from_ms_is_exact_milliseconds() {
    assert_eq!(
        ledger_repr::from_ms(Moment::from_unix_seconds(JAN_15)).unwrap(),
        "1768471200000"
    );
    assert_eq!(
        ledger_repr::from_ms(moment(JAN_15, 125_000_000)).unwrap(),
        "1768471200125"
    );
    assert_eq!(
        ledger_repr::from_ms(Moment::from_unix_seconds(0)).unwrap(),
        "0"
    );
    // The largest representable instant: u64::MAX milliseconds.
    assert_eq!(
        ledger_repr::millis(moment(18_446_744_073_709_551, 615_000_000)),
        Ok(u64::MAX)
    );
}

#[test]
fn d01_typed_errors_name_the_service_rule() {
    assert_eq!(
        Moment::new(0, 1_000_000_000),
        Err(NanosOutOfRange {
            subsec_nanos: 1_000_000_000
        })
    );
    assert_eq!(
        catalog_repr::since(moment(JAN_15, 1)),
        Err(CatalogReprError::FractionalSeconds { subsec_nanos: 1 })
    );
    assert_eq!(
        catalog_repr::since(moment(JAN_15, 1))
            .unwrap_err()
            .to_string(),
        "Catalog `since` takes whole seconds; the instant is 1 ns past its second"
    );
    assert_eq!(
        ledger_repr::from_ms(moment(-1, 0)).unwrap_err().to_string(),
        "Ledger `from_ms` takes instants from the Unix epoch on; Unix second -1 is before it"
    );
    let boxed: Box<dyn std::error::Error> =
        Box::new(LedgerReprError::OutOfRange { unix_seconds: 1 });
    assert!(boxed.to_string().contains("unsigned 64-bit"));
}

#[tokio::test]
async fn d02_unrepresentable_values_fail_before_any_request() {
    let server = services().await;
    let (catalog, ledger) = (catalog(&server.url()), ledger(&server.url()));
    let catalog_since = |m| catalog_repr::activity(&catalog, ACCOUNT, Some(m)).map(|_| ());
    let ledger_from = |m| ledger_repr::entries(&ledger, ACCOUNT, Some(m)).map(|_| ());

    // Catalog: precision, then the four-digit-year range.
    assert_eq!(
        catalog_since(moment(JAN_15, 1)),
        Err(CatalogReprError::FractionalSeconds { subsec_nanos: 1 })
    );
    assert_eq!(
        catalog_since(moment(JAN_15, 999_999_999)),
        Err(CatalogReprError::FractionalSeconds {
            subsec_nanos: 999_999_999
        })
    );
    for seconds in [-62_167_219_201, 253_402_300_800, i64::MIN, i64::MAX] {
        assert_eq!(
            catalog_since(Moment::from_unix_seconds(seconds)),
            Err(CatalogReprError::YearOutOfRange {
                unix_seconds: seconds
            })
        );
    }
    // Pre-1970 whole seconds are valid for Catalog.
    assert!(catalog_since(Moment::from_unix_seconds(-86_400)).is_ok());

    // Ledger: precision, sign, then u64 range.
    assert_eq!(
        ledger_from(moment(JAN_15, 1)),
        Err(LedgerReprError::SubMillisecond { subsec_nanos: 1 })
    );
    assert_eq!(
        ledger_from(moment(JAN_15, 125_000_001)),
        Err(LedgerReprError::SubMillisecond {
            subsec_nanos: 125_000_001
        })
    );
    for (seconds, nanos) in [(-1, 0), (-1, 999_000_000), (i64::MIN, 0)] {
        assert_eq!(
            ledger_from(moment(seconds, nanos)),
            Err(LedgerReprError::BeforeEpoch {
                unix_seconds: seconds
            })
        );
    }
    for (seconds, nanos) in [
        (18_446_744_073_709_551, 616_000_000),
        (18_446_744_073_709_552, 0),
        (i64::MAX, 0),
    ] {
        assert_eq!(
            ledger_from(moment(seconds, nanos)),
            Err(LedgerReprError::OutOfRange {
                unix_seconds: seconds
            })
        );
    }

    assert_eq!(server.count(), 0, "no rejected value reached a service");
}

#[tokio::test]
async fn d02_a_server_side_failure_is_not_a_preflight_failure() {
    let server = services().await;
    let ledger = ledger(&server.url());
    let since = Some(Moment::from_unix_seconds(JAN_15));

    // Representable values, but the caller sets a page size the service refuses.
    let error = ledger_repr::entries(&ledger, ACCOUNT, since)
        .unwrap()
        .set_page_size(0)
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status(), Some(reqwest::StatusCode::BAD_REQUEST));
    assert_eq!(server.count(), 1);
    assert_eq!(
        server.single().query(),
        Some("account=acct-002a&from_ms=1768471200000&page_size=0")
    );

    // The same distinction, as report outcomes.
    let failed = Outcome::attempt(
        ledger_repr::entries(&ledger, ACCOUNT, since).map(|r| r.set_page_size(0)),
        LedgerEntries::send,
    )
    .await;
    assert!(
        matches!(&failed, Outcome::Failed(e) if e.status() == Some(reqwest::StatusCode::BAD_REQUEST))
    );
    assert_eq!(server.count(), 2);

    let rejected = Outcome::attempt(
        ledger_repr::entries(&ledger, ACCOUNT, Some(moment(-1, 0))).map(|r| r.set_page_size(0)),
        LedgerEntries::send,
    )
    .await;
    assert!(matches!(
        rejected,
        Outcome::Rejected(LedgerReprError::BeforeEpoch { unix_seconds: -1 })
    ));
    assert_eq!(server.count(), 2, "a rejected conversion sends nothing");
}

#[tokio::test]
async fn d03_one_service_rejecting_does_not_suppress_the_other() {
    let server = services().await;
    let (catalog, ledger) = (catalog(&server.url()), ledger(&server.url()));

    // 125 ms past the second: Ledger can represent it, Catalog cannot.
    let report = account_report(
        &catalog,
        &ledger,
        ACCOUNT,
        Some(moment(JAN_15, 125_000_000)),
    )
    .await;

    assert!(matches!(
        report.catalog,
        Outcome::Rejected(CatalogReprError::FractionalSeconds {
            subsec_nanos: 125_000_000
        })
    ));
    let Outcome::Succeeded(page) = report.ledger else {
        panic!("Ledger should answer: {:?}", report.ledger);
    };
    let ids: Vec<_> = page.data.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "entry-002",
            "entry-003",
            "entry-004",
            "entry-005",
            "entry-006"
        ]
    );
    assert_eq!(page.data[0].at_ms, 1_768_471_200_125);
    let seen = server.single();
    assert_eq!(seen.path(), "/ledger/v1/entries");
    assert_eq!(
        seen.query(),
        Some("account=acct-002a&from_ms=1768471200125&page_size=10")
    );
}

#[tokio::test]
async fn d03_both_services_rejecting_send_nothing() {
    let server = services().await;
    let (catalog, ledger) = (catalog(&server.url()), ledger(&server.url()));

    // One nanosecond more: neither whole seconds nor whole milliseconds.
    let report = account_report(
        &catalog,
        &ledger,
        ACCOUNT,
        Some(moment(JAN_15, 125_000_001)),
    )
    .await;

    assert!(matches!(
        report.catalog,
        Outcome::Rejected(CatalogReprError::FractionalSeconds {
            subsec_nanos: 125_000_001
        })
    ));
    assert!(matches!(
        report.ledger,
        Outcome::Rejected(LedgerReprError::SubMillisecond {
            subsec_nanos: 125_000_001
        })
    ));
    assert_eq!(server.count(), 0);
}

#[tokio::test]
async fn d03_both_services_answer_a_whole_second() {
    let server = services().await;
    let (catalog, ledger) = (catalog(&server.url()), ledger(&server.url()));

    let report = account_report(
        &catalog,
        &ledger,
        ACCOUNT,
        Some(Moment::from_unix_seconds(1_769_904_000)),
    )
    .await;

    let Outcome::Succeeded(activity) = report.catalog else {
        panic!("Catalog should answer: {:?}", report.catalog);
    };
    let Outcome::Succeeded(entries) = report.ledger else {
        panic!("Ledger should answer: {:?}", report.ledger);
    };
    let ids: Vec<_> = activity.events.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["event-002", "event-003"]);
    let ids: Vec<_> = entries.data.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["entry-003", "entry-004", "entry-005", "entry-006"]);
    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].query_all("since"), ["2026-02-01T00:00:00Z"]);
    assert_eq!(seen[1].query_all("from_ms"), ["1769904000000"]);
}

#[tokio::test]
async fn d03_one_service_failing_keeps_the_other_answer() {
    // Catalog answers as usual; Ledger is unavailable.
    let catalog_service = catalog_service("/catalog/v1");
    let server = Server::start(move |request| {
        if request.path().starts_with("/ledger/") {
            Reply::problem(503, "UNAVAILABLE", "ledger is down")
        } else {
            catalog_service(request)
        }
    })
    .await;
    let (catalog, ledger) = (catalog(&server.url()), ledger(&server.url()));

    let report = account_report(&catalog, &ledger, ACCOUNT, None).await;

    let Outcome::Succeeded(activity) = report.catalog else {
        panic!("Catalog should answer: {:?}", report.catalog);
    };
    assert_eq!(activity.events.len(), 3);
    assert!(matches!(
        report.ledger,
        Outcome::Failed(ref e) if e.status() == Some(reqwest::StatusCode::SERVICE_UNAVAILABLE)
    ));
    assert_eq!(server.count(), 2);
}
