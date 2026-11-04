//! SF-A — schema-aware CRM extraction, end to end against the loopback server.
//!
//! Fixtures: `tests/fixtures/sf_a/` (see `tests/fixtures/MANIFEST.json`). Command:
//! `cargo test -p concord-example-salesforce-app --test sf_a_crm_extraction`.

mod support;

use std::num::NonZeroUsize;

use concord_example_salesforce_app::sf_a::{
    Completion, ExtractionError, ExtractionInput, crm_extraction,
};
use concord_example_salesforce_sdk::ids::{BatchSize, RecordId};
use concord_example_salesforce_sdk::model::Field;
use concord_example_salesforce_sdk::{LinkErrorKind, ProtocolErrorKind, SfError};
use concord_example_testserver::{Recorded, Reply};
use concord_pagination::{Budget, Resume};
use support::*;

const ACCOUNTS_SOQL: &str =
    "SELECT Id, Name, Industry, AnnualRevenue, Customer_Tier__c FROM Account ORDER BY Id ASC";
const CONTACTS_SOQL_A: &str = "SELECT Id, LastName, FirstName, Email, AccountId, Loyalty_Tier__c, \
                               Account.Name FROM Contact ORDER BY Id ASC";
const CONTACTS_SOQL_B: &str =
    "SELECT Id, LastName, FirstName, Email, AccountId, Account.Name FROM Contact ORDER BY Id ASC";
const L1: &str = "01gA0000000AbCdEAA-2";
const L2: &str = "01gA0000000AbCdEAA-4";
const LC: &str = "01gA0000000CoNtAAA-2";

fn budget(n: usize) -> Budget {
    Budget::new(NonZeroUsize::new(n).unwrap())
}

fn q(request: &Recorded) -> String {
    let all = request.query_all("q");
    assert_eq!(all.len(), 1, "exactly one q parameter");
    all[0].clone()
}

/// `GET query?q=`: first batches by statement.
fn first_batches(
    accounts: &'static str,
    contacts: &'static str,
) -> impl FnMut(&Recorded) -> Reply + Send {
    move |request| {
        let soql = q(request);
        if soql.contains(" FROM Account ") {
            Reply::json(200, fixture(accounts))
        } else if soql.contains(" FROM Contact ") {
            Reply::json(200, fixture(contacts))
        } else {
            Reply::text(400, "text/plain", "unexpected statement")
        }
    }
}

fn org_a_routes(router: &Router) {
    router
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Account/describe/"),
            json(fixture("sf_a/describe_account.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Contact/describe/"),
            json(fixture("sf_a/describe_contact.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{L1}")),
            json(fixture("sf_a/accounts_page_2.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{L2}")),
            json(fixture("sf_a/accounts_page_3.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{LC}")),
            json(fixture("sf_a/contacts_page_2.json")),
        );
}

#[tokio::test]
async fn sf_a_multi_page_extraction_with_schema_custom_fields_and_record_read() {
    let router = Router::new();
    org_a_routes(&router);
    router.on(
        ORG_A,
        "GET",
        &api("query"),
        first_batches("sf_a/accounts_page_1.json", "sf_a/contacts_page_1.json"),
    );
    let read_id = "001A00000000001AAA";
    router.on(
        ORG_A,
        "GET",
        &api(&format!(
            "sobjects/Account/{read_id}?fields=Id%2CName%2CIndustry%2CAnnualRevenue%2CCustomer_Tier__c"
        )),
        json(fixture("sf_a/account_record.json")),
    );
    let server = router.start().await;
    let ctx = ctx(ORG_A, &server);

    let mut input = ExtractionInput::new(budget(10));
    input.batch_size = Some(BatchSize::new(200).unwrap());
    input.account_reads = vec![RecordId::new(read_id).unwrap()];
    let report = crm_extraction(&ctx, &input).await.unwrap();

    // Schema provenance: selected fields from describe, the custom field included.
    assert_eq!(report.account_schema.api_version.to_string(), "68.0");
    let custom: Vec<_> = report
        .account_schema
        .selected
        .iter()
        .filter(|f| f.custom)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(custom, ["Customer_Tier__c"]);
    assert!(report.account_schema.missing_optional.is_empty());
    assert_eq!(report.accounts.soql.as_str(), ACCOUNTS_SOQL);
    assert_eq!(report.contacts.soql.as_str(), CONTACTS_SOQL_A);

    // Three Account pages, two Contact pages; totalSize kept as metadata.
    assert!(matches!(report.accounts.completion, Completion::Complete));
    assert!(matches!(report.contacts.completion, Completion::Complete));
    assert_eq!(report.accounts.pages.len(), 3);
    assert_eq!(report.contacts.pages.len(), 2);
    assert!(report.accounts.pages.iter().all(|p| p.total_size == 5));
    let ids: Vec<_> = report
        .accounts
        .records
        .iter()
        .map(|a| a.id.to_string())
        .collect();
    assert_eq!(ids.len(), 5);
    assert_eq!(ids[0], "001A00000000001AAA");
    assert_eq!(ids[4], "001A00000000005AAA");

    // Optional and custom fields: value, null.
    let acme = &report.accounts.records[0];
    assert_eq!(acme.name, "Acme \"Rockets\"");
    assert_eq!(acme.customer_tier, Field::Value("Gold".to_owned()));
    assert_eq!(acme.annual_revenue, Field::Value(1_200_000.5));
    let brook = &report.accounts.records[1];
    assert_eq!(brook.industry, Field::Null);
    assert_eq!(brook.customer_tier, Field::Null);
    let zoe = &report.contacts.records[0];
    assert_eq!(zoe.loyalty_tier, Field::Value("Gold".to_owned()));
    // The relationship path stays dynamic.
    assert_eq!(zoe.extra["Account"]["Name"], "Acme \"Rockets\"");
    let doe = &report.contacts.records[1];
    assert_eq!(doe.first_name, Field::Null);
    assert_eq!(doe.account_id, Field::Null);

    // The record read.
    assert_eq!(report.account_reads.len(), 1);
    assert!(report.account_reads[0].1.is_ok());
    assert!(report.is_complete());

    // Wire: statements as the q value, locators as paths, batch size header, one org.
    let requests = server.requests();
    assert_eq!(router.unrouted(), 0);
    assert_org_isolation(&requests);
    let queries: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == api("query"))
        .collect();
    assert_eq!(queries.len(), 2);
    assert_eq!(q(queries[0]), ACCOUNTS_SOQL);
    assert_eq!(q(queries[1]), CONTACTS_SOQL_A);
    for r in requests
        .iter()
        .filter(|r| r.path().starts_with(&api("query")))
    {
        assert_eq!(r.header("sforce-query-options"), Some("batchSize=200"));
        assert_eq!(r.header("accept"), Some("application/json"));
    }
    let mores: Vec<_> = requests
        .iter()
        .filter(|r| r.path().starts_with(&api("query/")))
        .map(|r| r.target.clone())
        .collect();
    assert_eq!(
        mores,
        [
            api(&format!("query/{L1}")),
            api(&format!("query/{L2}")),
            api(&format!("query/{LC}"))
        ]
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn sf_a_user_data_is_a_literal_and_encoded_by_the_transport() {
    let router = Router::new();
    org_a_routes(&router);
    router.on(
        ORG_A,
        "GET",
        &api("query"),
        first_batches("sf_a/accounts_page_3.json", "sf_a/contacts_page_2.json"),
    );
    let server = router.start().await;
    let ctx = ctx(ORG_A, &server);
    let mut input = ExtractionInput::new(budget(5));
    input.industry = Some("O'Brien & Sons' \"50%\"".to_owned());
    input.email_suffix = Some("_x%@corp.test".to_owned());
    crm_extraction(&ctx, &input).await.unwrap();
    let requests = server.requests();
    let queries: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == api("query"))
        .collect();
    assert_eq!(
        q(queries[0]),
        "SELECT Id, Name, Industry, AnnualRevenue, Customer_Tier__c FROM Account \
         WHERE Industry = 'O\\'Brien & Sons\\' \\\"50%\\\"' ORDER BY Id ASC"
    );
    assert_eq!(
        q(queries[1]),
        "SELECT Id, LastName, FirstName, Email, AccountId, Loyalty_Tier__c, Account.Name FROM Contact \
         WHERE Email LIKE '%\\_x\\%@corp.test' ORDER BY Id ASC"
    );
    // The raw target percent-encodes the statement (no raw quote, ampersand or space).
    let raw = queries[0].target.split_once("q=").unwrap().1;
    assert!(!raw.contains('\'') && !raw.contains(' ') && !raw.contains('"'));
    assert!(raw.contains("%26"), "{raw}");
    server.shutdown().await;
}

#[tokio::test]
async fn sf_a_two_orgs_stay_isolated_and_missing_custom_fields_are_reported() {
    let router = Router::new();
    org_a_routes(&router);
    router.on(
        ORG_A,
        "GET",
        &api("query"),
        first_batches("sf_a/accounts_page_1.json", "sf_a/contacts_page_1.json"),
    );
    router
        .on(
            ORG_B,
            "GET",
            &api("sobjects/Account/describe/"),
            json(fixture("sf_a/describe_account.json")),
        )
        .on(
            ORG_B,
            "GET",
            &api("sobjects/Contact/describe/"),
            json(fixture("sf_a/describe_contact_without_custom.json")),
        )
        .on(
            ORG_B,
            "GET",
            &api("query"),
            first_batches("sf_a/org_b_accounts.json", "sf_a/org_b_contacts.json"),
        );
    let server = router.start().await;
    let a = ctx(ORG_A, &server);
    let b = ctx(ORG_B, &server);
    let input = ExtractionInput::new(budget(10));
    let (ra, rb) = tokio::join!(crm_extraction(&a, &input), crm_extraction(&b, &input));
    let (ra, rb) = (ra.unwrap(), rb.unwrap());
    assert!(ra.is_complete() && rb.is_complete());
    assert_eq!(ra.contacts.soql.as_str(), CONTACTS_SOQL_A);
    assert_eq!(rb.contacts.soql.as_str(), CONTACTS_SOQL_B);
    assert_eq!(
        rb.contact_schema
            .missing_optional
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>(),
        ["Loyalty_Tier__c"]
    );
    // Not selected in org B: absent from the records, not null.
    assert!(
        rb.contacts
            .records
            .iter()
            .all(|c| c.loyalty_tier == Field::NotSelected)
    );
    assert_eq!(rb.accounts.records.len(), 1);
    assert_eq!(ra.accounts.records.len(), 5);

    let requests = server.requests();
    assert_eq!(router.unrouted(), 0);
    assert_org_isolation(&requests);
    assert!(requests.iter().any(|r| r.host.starts_with(ORG_B.host)));
    server.shutdown().await;
}

#[tokio::test]
async fn sf_a_expired_locator_after_earlier_pages_requires_an_explicit_restart() {
    let router = Router::new();
    router
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Account/describe/"),
            json(fixture("sf_a/describe_account.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Contact/describe/"),
            json(fixture("sf_a/describe_contact.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("query"),
            first_batches("sf_a/accounts_page_1.json", "sf_a/contacts_page_2.json"),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{L1}")),
            status_json(400, fixture("sf_a/error_invalid_query_locator.json")),
        );
    let server = router.start().await;
    let ctx = ctx(ORG_A, &server);
    let report = crm_extraction(&ctx, &ExtractionInput::new(budget(10)))
        .await
        .unwrap();
    // The first page's records are kept; the section must restart from its first batch.
    assert_eq!(report.accounts.records.len(), 2);
    match &report.accounts.completion {
        Completion::RestartRequired { error } => {
            assert!(error.is_expired_locator());
            let api = error.api().unwrap();
            assert_eq!(api.status().as_u16(), 400);
            assert_eq!(api.errors()[0].error_code, "INVALID_QUERY_LOCATOR");
            assert!(!error.to_string().contains(L1));
        }
        other => panic!("{other:?}"),
    }
    assert!(
        report.resume().accounts.is_none(),
        "no resume point from an expired locator"
    );
    assert!(matches!(report.contacts.completion, Completion::Complete));
    server.shutdown().await;
}

#[tokio::test]
async fn sf_a_transient_failure_resumes_from_the_failed_locator_without_duplicates() {
    let router = Router::new();
    router
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Account/describe/"),
            json(fixture("sf_a/describe_account.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Contact/describe/"),
            json(fixture("sf_a/describe_contact.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("query"),
            first_batches("sf_a/accounts_page_1.json", "sf_a/contacts_page_2.json"),
        )
        // Page 2 fails once with 503, then succeeds.
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{L1}")),
            status_json(503, fixture("sf_a/error_server_unavailable.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{L1}")),
            json(fixture("sf_a/accounts_page_2.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{L2}")),
            json(fixture("sf_a/accounts_page_3.json")),
        );
    let server = router.start().await;
    let ctx = ctx(ORG_A, &server);
    let first = crm_extraction(&ctx, &ExtractionInput::new(budget(10)))
        .await
        .unwrap();
    let resume = match &first.accounts.completion {
        Completion::Interrupted { resume, error } => {
            assert_eq!(error.api().unwrap().status().as_u16(), 503);
            assert!(matches!(resume.position(), Resume::At(_)));
            resume.clone()
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(first.accounts.records.len(), 2);

    let before = server.requests().len();
    let mut input = ExtractionInput::new(budget(10));
    input.resume = first.resume();
    let second = crm_extraction(&ctx, &input).await.unwrap();
    assert!(second.accounts.resumed);
    assert!(matches!(second.accounts.completion, Completion::Complete));
    // The resumed section starts at the failed batch, not at the statement.
    let resumed: Vec<_> = server.requests()[before..]
        .iter()
        .filter(|r| r.path().starts_with(&api("query")))
        .map(|r| r.target.clone())
        .collect();
    assert_eq!(resumed[0], api(&format!("query/{L1}")));
    let mut ids: Vec<_> = first
        .accounts
        .records
        .iter()
        .map(|a| a.id.to_string())
        .collect();
    ids.extend(second.accounts.records.iter().map(|a| a.id.to_string()));
    let mut unique = ids.clone();
    unique.dedup();
    assert_eq!(ids.len(), 5);
    assert_eq!(unique, ids, "no duplicates, no gaps");
    drop(resume);
    server.shutdown().await;
}

#[tokio::test]
async fn sf_a_resume_points_are_bound_to_their_org_and_statement() {
    let router = Router::new();
    router
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Account/describe/"),
            json(fixture("sf_a/describe_account.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Contact/describe/"),
            json(fixture("sf_a/describe_contact.json")),
        )
        .on(
            ORG_B,
            "GET",
            &api("sobjects/Account/describe/"),
            json(fixture("sf_a/describe_account.json")),
        )
        .on(
            ORG_B,
            "GET",
            &api("sobjects/Contact/describe/"),
            json(fixture("sf_a/describe_contact.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("query"),
            first_batches("sf_a/accounts_page_1.json", "sf_a/contacts_page_2.json"),
        );
    let server = router.start().await;
    let a = ctx(ORG_A, &server);
    let bounded = crm_extraction(&a, &ExtractionInput::new(budget(1)))
        .await
        .unwrap();
    let resume = bounded.resume();
    assert!(matches!(
        bounded.accounts.completion,
        Completion::Bounded { .. }
    ));
    assert!(resume.accounts.is_some());

    // Another org.
    let mut input = ExtractionInput::new(budget(1));
    input.resume = resume.clone();
    let err = crm_extraction(&ctx(ORG_B, &server), &input)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ExtractionError::ResumeMismatch { section: "Account" }
    ));
    // Another statement (a filter changes it).
    input.industry = Some("Retail".to_owned());
    let err = crm_extraction(&a, &input).await.unwrap_err();
    assert!(matches!(err, ExtractionError::ResumeMismatch { .. }));
    server.shutdown().await;
}

#[tokio::test]
async fn sf_a_returned_links_are_validated_and_counts_never_make_locators() {
    let foreign = fixture("sf_a/accounts_page_1.json").replace(
        &format!("\"/services/data/v68.0/query/{L1}\""),
        &format!("\"http://{}/services/data/v68.0/query/{L1}\"", ORG_B.host),
    );
    let other_version = fixture("sf_a/accounts_page_1.json").replace("v68.0/query", "v67.0/query");
    let no_next = r#"{"totalSize":5000,"done":false,"records":[]}"#.to_owned();
    let done_with_next =
        fixture("sf_a/accounts_page_1.json").replace("\"done\": false", "\"done\": true");
    type Check = fn(&SfError) -> bool;
    let cases: [(String, Check); 4] = [
        (
            foreign,
            |e| matches!(e, SfError::Link(l) if l.kind() == LinkErrorKind::ForeignOrigin),
        ),
        (
            other_version,
            |e| matches!(e, SfError::Link(l) if l.kind() == LinkErrorKind::VersionMismatch),
        ),
        (
            no_next,
            |e| matches!(e, SfError::Protocol(p) if *p.kind() == ProtocolErrorKind::MissingNextLink),
        ),
        (
            done_with_next,
            |e| matches!(e, SfError::Protocol(p) if *p.kind() == ProtocolErrorKind::NextLinkWhenDone),
        ),
    ];
    for (body, expected) in cases {
        let router = Router::new();
        let accounts = body.clone();
        router
            .on(
                ORG_A,
                "GET",
                &api("sobjects/Account/describe/"),
                json(fixture("sf_a/describe_account.json")),
            )
            .on(
                ORG_A,
                "GET",
                &api("sobjects/Contact/describe/"),
                json(fixture("sf_a/describe_contact.json")),
            )
            .on(ORG_A, "GET", &api("query"), move |r: &Recorded| {
                if q(r).contains(" FROM Account ") {
                    Reply::json(200, accounts.clone())
                } else {
                    Reply::json(200, fixture("sf_a/contacts_page_2.json"))
                }
            });
        let server = router.start().await;
        let ctx = ctx(ORG_A, &server);
        let report = crm_extraction(&ctx, &ExtractionInput::new(budget(10)))
            .await
            .unwrap();
        match &report.accounts.completion {
            Completion::Interrupted { error, .. } => assert!(expected(error), "{error:?}"),
            other => panic!("{other:?}"),
        }
        // Nothing was requested beyond the refused page: no foreign host, no guessed locator.
        let requests = server.requests();
        assert!(requests.iter().all(|r| r.host.starts_with(ORG_A.host)));
        assert!(
            !requests
                .iter()
                .any(|r| r.path().starts_with(&api("query/")))
        );
        assert_eq!(router.unrouted(), 0);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn sf_a_diagnostics_never_show_tokens_locators_literals_or_record_values() {
    let router = Router::new();
    router
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Account/describe/"),
            json(fixture("sf_a/describe_account.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("sobjects/Contact/describe/"),
            json(fixture("sf_a/describe_contact.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api("query"),
            first_batches("sf_a/accounts_page_1.json", "sf_a/contacts_page_1.json"),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{L1}")),
            status_json(503, fixture("sf_a/error_server_unavailable.json")),
        )
        .on(
            ORG_A,
            "GET",
            &api(&format!("query/{LC}")),
            json(fixture("sf_a/contacts_page_2.json")),
        );
    let server = router.start().await;
    let ctx = ctx(ORG_A, &server);
    let mut input = ExtractionInput::new(budget(10));
    input.industry = Some("SecretIndustryLiteral".to_owned());
    // The industry filter changes the statement, so route by path: reuse the same pages.
    let report = crm_extraction(&ctx, &input).await.unwrap();
    let shown = format!("{report:?} {ctx:?}");
    for secret in [
        ORG_A.token,
        "SECRET",
        L1,
        "SecretIndustryLiteral",
        "Rockets",
        "Brook",
        "zoe@example.test",
        "Server temporarily unavailable",
    ] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    if let Completion::Interrupted { error, .. } = &report.accounts.completion {
        let text = format!("{error} {error:?}");
        assert!(
            text.contains("503") && text.contains("SERVER_UNAVAILABLE"),
            "{text}"
        );
        assert!(!text.contains(L1) && !text.contains("temporarily"));
    } else {
        panic!("{:?}", report.accounts.completion);
    }
    server.shutdown().await;
}
