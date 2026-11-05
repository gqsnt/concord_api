//! The frozen initial consumer ([`before`]) against a loopback org: exact requests (paths,
//! targets, headers, job body), typed rows across a `Sforce-Locator` chain in the comma dialect,
//! describe validation in an org without the custom field, two orgs with two tokens, the
//! result-set budget, and a typed error that does not render the token.
//!
//! The loopback org behaves as the Bulk API 2.0 documentation describes: a query job's result
//! sets list "the columns ... in the same order as you requested them" (from API 50.0), every
//! field quoted, in the job's `columnDelimiter` and `lineEnding`. Its CSV writer is written here,
//! independently of the SDK's encoder. Never edited after stage 1.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use concord::reqwest;
use concord_check_maintenance_salesforce::before::{
    MailingError, MailingExport, export_contacts, export_soql,
};
use concord_example_salesforce_sdk::OrgContext;
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::csv::CONTACT_EXPORT_FIELDS;
use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl, MaxRecords};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome, Resume};
use serde_json::{Value, json};

const HOST_A: &str = "org-a.my.salesforce.test";
const HOST_B: &str = "org-b.my.salesforce.test";
const HOST_C: &str = "org-c.my.salesforce.test";
const TOKEN_A: &str = "00DA00000000001!SECRETa";
const TOKEN_B: &str = "00DB00000000002!SECRETb";
const TOKEN_C: &str = "00DC00000000003!SECRETc";
const JOB: &str = "750A00000000EXPAAA";
const API: &str = "/services/data/v68.0";

/// The Contact fields of the loopback orgs' describe documents: (API name, type, custom).
const STANDARD_CONTACT_FIELDS: [(&str, &str, bool); 18] = [
    ("Id", "id", false),
    ("IsDeleted", "boolean", false),
    ("AccountId", "reference", false),
    ("LastName", "string", false),
    ("FirstName", "string", false),
    ("Salutation", "picklist", false),
    ("Name", "string", false),
    ("MailingCity", "string", false),
    ("Phone", "phone", false),
    ("Email", "email", false),
    ("Title", "string", false),
    ("Department", "string", false),
    ("LeadSource", "picklist", false),
    ("Birthdate", "date", false),
    ("HasOptedOutOfEmail", "boolean", false),
    ("DoNotCall", "boolean", false),
    ("OwnerId", "reference", false),
    ("CreatedDate", "datetime", false),
];
const CUSTOM_TIER: (&str, &str, bool) = ("Loyalty_Tier__c", "picklist", true);

/// Org A's Contacts (field values as Salesforce stores them; a field without a value here is
/// empty, or `false` for a boolean).
fn org_a_contacts() -> Vec<Vec<(&'static str, &'static str)>> {
    vec![
        vec![
            ("Id", "003A00000000001AAA"),
            ("FirstName", "Zoë"),
            ("LastName", "O'Brien, Jr."),
            ("Email", "zoe@example.test"),
            ("AccountId", "001A00000000001AAA"),
            ("Loyalty_Tier__c", "Gold"),
            ("Title", "VP, Sales"),
        ],
        vec![
            ("Id", "003A00000000002AAA"),
            ("LastName", "said \"hi\"\nthen, left"),
        ],
        vec![
            ("Id", "003A00000000003AAA"),
            ("FirstName", "Ann"),
            ("LastName", "Plain"),
            ("Email", "ann@example.test"),
            ("AccountId", "001A00000000002AAA"),
            ("Loyalty_Tier__c", "Silver, Plus"),
        ],
    ]
}

struct Org {
    host: &'static str,
    token: &'static str,
    fields: Vec<(&'static str, &'static str, bool)>,
    contacts: Vec<Vec<(&'static str, &'static str)>>,
}

#[derive(Default)]
struct JobState {
    columns: Vec<String>,
    dialect: (String, String),
    polls: u32,
    csv_bytes: u64,
}

fn orgs() -> Vec<Org> {
    let mut with_tier = STANDARD_CONTACT_FIELDS.to_vec();
    with_tier.push(CUSTOM_TIER);
    vec![
        Org {
            host: HOST_A,
            token: TOKEN_A,
            fields: with_tier,
            contacts: org_a_contacts(),
        },
        Org {
            host: HOST_B,
            token: TOKEN_B,
            fields: STANDARD_CONTACT_FIELDS.to_vec(),
            contacts: Vec::new(),
        },
    ]
}

fn job_doc(state: &str, dialect: &(String, String)) -> String {
    json!({
        "id": JOB, "operation": "query", "object": "Contact",
        "createdById": "005A00000000001AAA", "createdDate": "2026-11-05T10:00:00.000+0000",
        "systemModstamp": "2026-11-05T10:00:01.000+0000", "state": state,
        "concurrencyMode": "Parallel", "contentType": "CSV", "apiVersion": 68.0,
        "lineEnding": dialect.1, "columnDelimiter": dialect.0,
    })
    .to_string()
}

/// Salesforce-style CSV: every field quoted, quotes doubled, the job's delimiter and ending.
fn salesforce_csv(dialect: &(String, String), columns: &[String], rows: &[Vec<String>]) -> String {
    let delimiter = match dialect.0.as_str() {
        "COMMA" => ",",
        "SEMICOLON" => ";",
        "TAB" => "\t",
        "PIPE" => "|",
        "CARET" => "^",
        "BACKQUOTE" => "`",
        other => panic!("unknown delimiter {other}"),
    };
    let eol = if dialect.1 == "CRLF" { "\r\n" } else { "\n" };
    let line = |fields: &[String]| {
        let quoted: Vec<String> = fields
            .iter()
            .map(|f| format!("\"{}\"", f.replace('"', "\"\"")))
            .collect();
        format!("{}{eol}", quoted.join(delimiter))
    };
    let mut out = line(columns);
    for row in rows {
        out.push_str(&line(row));
    }
    out
}

fn respond(orgs: &[Org], jobs: &Mutex<HashMap<&'static str, JobState>>, r: &Recorded) -> Reply {
    if r.host.starts_with(HOST_C) {
        return Reply::json(
            401,
            r#"[{"message":"Session expired or invalid","errorCode":"INVALID_SESSION_ID"}]"#,
        );
    }
    let org = orgs
        .iter()
        .find(|o| r.host.starts_with(o.host))
        .expect("a known org host");
    assert_eq!(
        r.header("authorization"),
        Some(format!("Bearer {}", org.token).as_str()),
        "{} {} on {}",
        r.method,
        r.target,
        org.host
    );
    let path = r.path();
    let mut jobs = jobs.lock().unwrap();
    if r.method == "GET" && path == format!("{API}/sobjects/Contact/describe/") {
        let fields: Vec<Value> = org
            .fields
            .iter()
            .map(|(name, kind, custom)| {
                json!({"name": name, "type": kind, "custom": custom, "nillable": *kind != "boolean"})
            })
            .collect();
        return Reply::json(
            200,
            json!({"name": "Contact", "label": "Contact", "custom": false, "queryable": true,
                   "keyPrefix": "003", "fields": fields})
            .to_string(),
        );
    }
    if r.method == "POST" && path == format!("{API}/jobs/query") {
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        let query = body["query"].as_str().unwrap();
        let select = query
            .strip_prefix("SELECT ")
            .and_then(|q| q.split_once(" FROM "))
            .unwrap()
            .0;
        let columns: Vec<String> = select.split(", ").map(str::to_owned).collect();
        for column in &columns {
            assert!(
                org.fields.iter().any(|(name, _, _)| name == column),
                "{column} queried in an org that lacks it"
            );
        }
        let dialect = (
            body["columnDelimiter"].as_str().unwrap().to_owned(),
            body["lineEnding"].as_str().unwrap().to_owned(),
        );
        let doc = job_doc("UploadComplete", &dialect);
        jobs.insert(
            org.host,
            JobState {
                columns,
                dialect,
                ..JobState::default()
            },
        );
        return Reply::json(200, doc);
    }
    let job = jobs.get_mut(org.host).expect("a created job");
    if r.method == "GET" && path == format!("{API}/jobs/query/{JOB}") {
        job.polls += 1;
        let state = if job.polls == 1 {
            "InProgress"
        } else {
            "JobComplete"
        };
        return Reply::json(200, job_doc(state, &job.dialect));
    }
    if r.method == "GET" && path == format!("{API}/jobs/query/{JOB}/results") {
        let set: usize = match r.query_all("locator").first() {
            None => 0,
            Some(locator) => locator.strip_prefix("c2V0-").unwrap().parse().unwrap(),
        };
        let max = match r.query_all("maxRecords").first() {
            None => org.contacts.len().max(1),
            Some(max) => max.parse().unwrap(),
        };
        let start = (set * max).min(org.contacts.len());
        let end = ((set + 1) * max).min(org.contacts.len());
        let rows: Vec<Vec<String>> = org.contacts[start..end]
            .iter()
            .map(|contact| {
                job.columns
                    .iter()
                    .map(|column| {
                        let stored = contact.iter().find(|(name, _)| name == column);
                        let boolean = org
                            .fields
                            .iter()
                            .any(|(name, kind, _)| name == column && *kind == "boolean");
                        match stored {
                            Some((_, value)) => (*value).to_owned(),
                            None if boolean => "false".to_owned(),
                            None => String::new(),
                        }
                    })
                    .collect()
            })
            .collect();
        let body = salesforce_csv(&job.dialect, &job.columns, &rows);
        println!("{} result set {set} on the wire: {body:?}", org.host);
        job.csv_bytes += body.len() as u64;
        let next = if end < org.contacts.len() {
            format!("c2V0-{}", set + 1)
        } else {
            "null".to_owned()
        };
        return Reply::text(200, "text/csv; charset=UTF-8", body)
            .header("Sforce-Locator", next)
            .header("Sforce-NumberOfRecords", rows.len().to_string());
    }
    panic!("unexpected request {} {}", r.method, r.target);
}

type Jobs = Arc<Mutex<HashMap<&'static str, JobState>>>;

async fn start() -> (TestServer, Jobs) {
    let orgs = orgs();
    let jobs: Jobs = Arc::default();
    let state = Arc::clone(&jobs);
    let server = TestServer::start(move |r: &Recorded| respond(&orgs, &state, r)).await;
    (server, jobs)
}

fn context(server: &TestServer, host: &str, token: &str) -> OrgContext {
    let http = reqwest::Client::builder()
        .resolve(host, server.addr())
        .build()
        .unwrap();
    OrgContext::new(
        http,
        InstanceUrl::parse(&format!("http://{host}:{}", server.addr().port())).unwrap(),
        ApiVersion::PINNED,
        AccessToken::new(token).unwrap(),
    )
    .unwrap()
}

fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}

async fn no_wait(_: u32) {}

/// The deployment's export (comma / LF), filtered, two records per result set.
fn deployment_export() -> MailingExport {
    let mut export = MailingExport::new(Budget::new(n(10)), n(5));
    export.email_suffix = Some("@example.test".to_owned());
    export.limit = Some(500);
    export.max_records = Some(MaxRecords::new(2).unwrap());
    export
}

fn requests_to(server: &TestServer, host: &str) -> Vec<Recorded> {
    server
        .requests()
        .into_iter()
        .filter(|r| r.host.starts_with(host))
        .collect()
}

#[tokio::test]
async fn comma_export_reads_typed_rows_across_result_sets() {
    let (server, jobs) = start().await;
    let ctx = context(&server, HOST_A, TOKEN_A);
    let export = deployment_export();
    assert_eq!(
        export.dialect,
        BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf)
    );

    let report = export_contacts(&ctx, &export, no_wait).await.unwrap();

    // The requests, in order, exactly.
    let requests = requests_to(&server, HOST_A);
    let targets: Vec<(&str, &str)> = requests
        .iter()
        .map(|r| (r.method.as_str(), r.target.as_str()))
        .collect();
    let describe = format!("{API}/sobjects/Contact/describe/");
    let jobs_path = format!("{API}/jobs/query");
    let job = format!("{API}/jobs/query/{JOB}");
    let first = format!("{API}/jobs/query/{JOB}/results?maxRecords=2");
    let second = format!("{API}/jobs/query/{JOB}/results?locator=c2V0-1&maxRecords=2");
    assert_eq!(
        targets,
        [
            ("GET", describe.as_str()),
            ("POST", jobs_path.as_str()),
            ("GET", job.as_str()),
            ("GET", job.as_str()),
            ("GET", first.as_str()),
            ("GET", second.as_str()),
        ]
    );
    for r in &requests {
        assert_eq!(
            r.header("authorization"),
            Some(format!("Bearer {TOKEN_A}").as_str())
        );
    }
    assert_eq!(requests[0].header("accept"), Some("application/json"));
    assert_eq!(requests[4].header("accept"), Some("text/csv"));
    assert_eq!(requests[5].header("accept"), Some("text/csv"));

    // The job body: the export columns as the select list, the filter as an escaped literal,
    // the dialect stated explicitly.
    let soql = format!(
        "SELECT {} FROM Contact WHERE Email LIKE '%@example.test' LIMIT 500",
        CONTACT_EXPORT_FIELDS.join(", ")
    );
    assert_eq!(export_soql(&export).unwrap().as_str(), soql);
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    println!("job body on the wire: {}", requests[1].body_text());
    assert_eq!(
        body,
        json!({"operation": "query", "query": soql, "contentType": "CSV",
               "columnDelimiter": "COMMA", "lineEnding": "LF"})
    );
    assert_eq!(requests[1].header("content-type"), Some("application/json"));

    // Describe validation: every export column, as described, in select-list order.
    let names: Vec<&str> = report.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, CONTACT_EXPORT_FIELDS);
    let tier = report
        .columns
        .iter()
        .find(|c| c.name == "Loyalty_Tier__c")
        .unwrap();
    assert!(tier.custom);
    assert_eq!(tier.field_type, "picklist");

    // Typed rows: quoted commas, quotes and newlines in the comma dialect; empty is `None`.
    assert_eq!(report.job.as_str(), JOB);
    assert_eq!(report.dialect, export.dialect);
    assert_eq!(report.result_sets, 2);
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert!(report.resume.is_none());
    assert_eq!(report.rows.len(), 3);
    let zoe = &report.rows[0];
    assert_eq!(zoe.id.as_str(), "003A00000000001AAA");
    assert_eq!(zoe.first_name.as_deref(), Some("Zoë"));
    assert_eq!(zoe.last_name, "O'Brien, Jr.");
    assert_eq!(zoe.email.as_deref(), Some("zoe@example.test"));
    assert_eq!(
        zoe.account_id.as_ref().map(|id| id.as_str()),
        Some("001A00000000001AAA")
    );
    assert_eq!(zoe.loyalty_tier.as_deref(), Some("Gold"));
    let quiet = &report.rows[1];
    assert_eq!(quiet.last_name, "said \"hi\"\nthen, left");
    assert_eq!(quiet.first_name, None);
    assert_eq!(quiet.email, None);
    assert_eq!(quiet.account_id, None);
    assert_eq!(quiet.loyalty_tier, None);
    assert_eq!(report.rows[2].loyalty_tier.as_deref(), Some("Silver, Plus"));

    // The summary a mailing tool uses.
    let tiers = report.by_loyalty_tier();
    assert_eq!(tiers.get(&Some("Gold".to_owned())), Some(&1));
    assert_eq!(tiers.get(&Some("Silver, Plus".to_owned())), Some(&1));
    assert_eq!(tiers.get(&None), Some(&1));
    let unmailable: Vec<&str> = report
        .without_email()
        .iter()
        .map(|id| id.as_str())
        .collect();
    assert_eq!(unmailable, ["003A00000000002AAA"]);
    assert_eq!(
        report.bytes_read,
        jobs.lock().unwrap().get(HOST_A).unwrap().csv_bytes
    );

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn an_org_without_the_custom_field_is_refused_before_a_job_and_orgs_stay_apart() {
    let (server, _) = start().await;
    let ctx_a = context(&server, HOST_A, TOKEN_A);
    let ctx_b = context(&server, HOST_B, TOKEN_B);
    let export = deployment_export();

    let (a, b) = tokio::join!(
        export_contacts(&ctx_a, &export, no_wait),
        export_contacts(&ctx_b, &export, no_wait)
    );
    assert_eq!(a.unwrap().rows.len(), 3);
    match b {
        Err(MailingError::MissingFields(fields)) => assert_eq!(fields, ["Loyalty_Tier__c"]),
        other => panic!("{other:?}"),
    }

    // Org B saw its describe call and nothing else; every request carried its org's token
    // (the loopback org asserts it on every request, too).
    let b_requests = requests_to(&server, HOST_B);
    assert_eq!(b_requests.len(), 1);
    assert_eq!(
        b_requests[0].target,
        format!("{API}/sobjects/Contact/describe/")
    );
    assert_eq!(
        b_requests[0].header("authorization"),
        Some(format!("Bearer {TOKEN_B}").as_str())
    );
    assert_eq!(requests_to(&server, HOST_A).len(), 6);
    assert!(
        requests_to(&server, HOST_A)
            .iter()
            .all(|r| r.header("authorization") == Some(format!("Bearer {TOKEN_A}").as_str()))
    );

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn the_result_set_budget_bounds_the_export_with_a_resume_point() {
    let (server, _) = start().await;
    let ctx = context(&server, HOST_A, TOKEN_A);
    let mut export = deployment_export();
    export.set_budget = Budget::new(n(1));

    let report = export_contacts(&ctx, &export, no_wait).await.unwrap();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.result_sets, 1);
    assert_eq!(report.rows.len(), 2);
    match &report.resume {
        Some(Resume::At(locator)) => assert_eq!(locator.expose(), "c2V0-1"),
        other => panic!("{other:?}"),
    }
    let results = requests_to(&server, HOST_A)
        .into_iter()
        .filter(|r| r.path().ends_with("/results"))
        .count();
    assert_eq!(results, 1);

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn an_expired_session_is_a_typed_error_that_does_not_render_the_token() {
    let (server, _) = start().await;
    let ctx = context(&server, HOST_C, TOKEN_C);

    let error = export_contacts(&ctx, &deployment_export(), no_wait)
        .await
        .unwrap_err();
    match &error {
        MailingError::Describe(e) => {
            assert!(e.has_error_code("INVALID_SESSION_ID"));
            assert_eq!(e.api().unwrap().status().as_u16(), 401);
        }
        other => panic!("{other:?}"),
    }
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(TOKEN_C), "{rendered}");
    assert!(!rendered.contains("SECRET"), "{rendered}");
    assert_eq!(requests_to(&server, HOST_C).len(), 1);

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}
