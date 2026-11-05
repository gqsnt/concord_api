//! The updated consumer ([`after`]) against a loopback org: the export in the tab / CRLF job
//! dialect with the `HasOptedOutOfEmail` column — exact job body, typed rows with tabs, quotes and
//! CRLF inside values, the opt-out rule, the mailing file's exact bytes — a job document whose
//! dialect contradicts the requested one, and an opt-out value that is not a boolean.
//!
//! The loopback org is the frozen test's (copied, not shared, so `tests/before.rs` stays frozen):
//! result sets list the requested columns in order, every field quoted, in the job's
//! `columnDelimiter` and `lineEnding`, written independently of the SDK's encoder.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use concord::reqwest;
use concord::stream::ReadErrorKind;
use concord_check_maintenance_salesforce::after::{MAILING_DIALECT, mailing_export, mailing_list};
use concord_check_maintenance_salesforce::before::MailingError;
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl, MaxRecords};
use concord_example_salesforce_sdk::{OrgContext, ProtocolErrorKind, SfError};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome};
use serde_json::{Value, json};

const HOST_A: &str = "org-a.my.salesforce.test";
const HOST_D: &str = "org-d.my.salesforce.test";
const HOST_E: &str = "org-e.my.salesforce.test";
const TOKEN_A: &str = "00DA00000000001!SECRETa";
const TOKEN_D: &str = "00DD00000000004!SECRETd";
const TOKEN_E: &str = "00DE00000000005!SECRETe";
const JOB: &str = "750A00000000EXPAAA";
const API: &str = "/services/data/v68.0";

/// The Contact fields of the loopback orgs' describe documents: (API name, type, custom).
const CONTACT_FIELDS: [(&str, &str, bool); 19] = [
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
    ("Loyalty_Tier__c", "picklist", true),
];

type Contact = Vec<(&'static str, &'static str)>;

fn org_a_contacts() -> Vec<Contact> {
    vec![
        vec![
            ("Id", "003A00000000001AAA"),
            ("FirstName", "Zoë"),
            ("LastName", "O'Brien, Jr."),
            ("Email", "zoe@example.test"),
            ("AccountId", "001A00000000001AAA"),
            ("Loyalty_Tier__c", "Gold"),
            ("HasOptedOutOfEmail", "false"),
        ],
        vec![
            ("Id", "003A00000000002AAA"),
            ("LastName", "said \"hi\""),
            ("Email", "hi@example.test"),
            ("HasOptedOutOfEmail", "true"),
        ],
        vec![
            ("Id", "003A00000000003AAA"),
            ("LastName", "crlf\r\ninside"),
            ("HasOptedOutOfEmail", "false"),
        ],
        vec![
            ("Id", "003A00000000004AAA"),
            ("FirstName", "Ann"),
            ("LastName", "Plain\t\"Tabbed\""),
            ("Email", "ann@example.test"),
            ("AccountId", "001A00000000002AAA"),
            ("Loyalty_Tier__c", "Silver, Plus"),
            ("HasOptedOutOfEmail", "false"),
        ],
    ]
}

struct Org {
    host: &'static str,
    token: &'static str,
    contacts: Vec<Contact>,
    /// Job documents state this dialect instead of the requested one.
    echo_dialect: Option<(&'static str, &'static str)>,
}

#[derive(Default)]
struct JobState {
    columns: Vec<String>,
    dialect: (String, String),
    polls: u32,
    csv_bytes: u64,
}

fn orgs() -> Vec<Org> {
    vec![
        Org {
            host: HOST_A,
            token: TOKEN_A,
            contacts: org_a_contacts(),
            echo_dialect: None,
        },
        Org {
            host: HOST_D,
            token: TOKEN_D,
            contacts: org_a_contacts(),
            echo_dialect: Some(("COMMA", "LF")),
        },
        Org {
            host: HOST_E,
            token: TOKEN_E,
            contacts: vec![vec![
                ("Id", "003E00000000001AAA"),
                ("LastName", "Unknown"),
                ("Email", "unknown@example.test"),
                ("HasOptedOutOfEmail", ""),
            ]],
            echo_dialect: None,
        },
    ]
}

fn job_doc(state: &str, dialect: (&str, &str)) -> String {
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
        let fields: Vec<Value> = CONTACT_FIELDS
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
        let dialect = (
            body["columnDelimiter"].as_str().unwrap().to_owned(),
            body["lineEnding"].as_str().unwrap().to_owned(),
        );
        let stated = org.echo_dialect.unwrap_or((&dialect.0, &dialect.1));
        let doc = job_doc("UploadComplete", stated);
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
        let stated = org.echo_dialect.unwrap_or((&job.dialect.0, &job.dialect.1));
        return Reply::json(200, job_doc(state, stated));
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
                        contact
                            .iter()
                            .find(|(name, _)| name == column)
                            .map(|(_, value)| (*value).to_owned())
                            .unwrap_or_default()
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

fn requests_to(server: &TestServer, host: &str) -> Vec<Recorded> {
    server
        .requests()
        .into_iter()
        .filter(|r| r.host.starts_with(host))
        .collect()
}

/// The deployment's export after the change, filtered, two records per result set.
fn deployment_export() -> concord_check_maintenance_salesforce::before::MailingExport {
    let mut export = mailing_export(Budget::new(n(10)), n(5));
    export.email_suffix = Some("@example.test".to_owned());
    export.limit = Some(500);
    export.max_records = Some(MaxRecords::new(2).unwrap());
    export
}

#[tokio::test]
async fn tab_crlf_mailing_list_reads_and_writes_the_opt_out_column() {
    let (server, jobs) = start().await;
    let ctx = context(&server, HOST_A, TOKEN_A);
    let export = deployment_export();
    assert_eq!(
        export.dialect,
        BulkDialect::new(ColumnDelimiter::Tab, LineEnding::Crlf)
    );

    let list = mailing_list(&ctx, &export, no_wait).await.unwrap();

    // The requests: the frozen consumer's sequence, in the new dialect.
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
    assert_eq!(requests[4].header("accept"), Some("text/csv"));

    // The job body, byte for byte: the new column in the select list, the new dialect.
    assert_eq!(
        requests[1].body_text(),
        "{\"operation\":\"query\",\"query\":\"SELECT Id, FirstName, LastName, Email, AccountId, \
         Loyalty_Tier__c, HasOptedOutOfEmail FROM Contact WHERE Email LIKE '%@example.test' \
         LIMIT 500\",\"contentType\":\"CSV\",\"columnDelimiter\":\"TAB\",\"lineEnding\":\"CRLF\"}"
    );

    // Describe validation now covers the new column.
    let opt_out = list
        .columns
        .iter()
        .find(|c| c.name == "HasOptedOutOfEmail")
        .unwrap();
    assert_eq!(opt_out.field_type, "boolean");
    assert!(!opt_out.custom);
    assert_eq!(list.columns.len(), 7);

    // The opt-out rule over typed rows parsed in TAB / CRLF.
    assert_eq!(list.job.as_str(), JOB);
    assert_eq!(list.dialect, MAILING_DIALECT);
    assert_eq!(list.outcome, Outcome::Exhausted);
    let ids = |v: &[concord_example_salesforce_sdk::ids::RecordId]| -> Vec<String> {
        v.iter().map(|id| id.as_str().to_owned()).collect()
    };
    assert_eq!(ids(&list.opted_out), ["003A00000000002AAA"]);
    assert_eq!(ids(&list.without_email), ["003A00000000003AAA"]);
    let recipients: Vec<&str> = list.recipients.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(recipients, ["003A00000000001AAA", "003A00000000004AAA"]);
    assert!(!list.recipients[0].has_opted_out_of_email);
    assert_eq!(list.recipients[1].last_name, "Plain\t\"Tabbed\"");
    assert_eq!(
        list.recipients[1].loyalty_tier.as_deref(),
        Some("Silver, Plus")
    );
    assert!(jobs.lock().unwrap().get(HOST_A).unwrap().csv_bytes > 0);

    // The mailing file, in the job's dialect: tabs and quotes quoted, commas not.
    let file = list.to_csv().unwrap();
    assert_eq!(
        String::from_utf8(file).unwrap(),
        "Id\tFirstName\tLastName\tEmail\tAccountId\tLoyalty_Tier__c\tHasOptedOutOfEmail\r\n\
         003A00000000001AAA\tZoë\tO'Brien, Jr.\tzoe@example.test\t001A00000000001AAA\tGold\tfalse\r\n\
         003A00000000004AAA\tAnn\t\"Plain\t\"\"Tabbed\"\"\"\tann@example.test\t001A00000000002AAA\t\
         Silver, Plus\tfalse\r\n"
    );

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn a_job_document_that_contradicts_the_requested_dialect_is_refused() {
    let (server, _) = start().await;
    let ctx = context(&server, HOST_D, TOKEN_D);

    let error = mailing_list(&ctx, &deployment_export(), no_wait)
        .await
        .unwrap_err();
    match &error {
        MailingError::Job(SfError::Protocol(p)) => {
            assert_eq!(*p.kind(), ProtocolErrorKind::DialectChanged);
        }
        other => panic!("{other:?}"),
    }
    // Refused on the creation document: no poll, no result set.
    let targets: Vec<String> = requests_to(&server, HOST_D)
        .into_iter()
        .map(|r| format!("{} {}", r.method, r.target))
        .collect();
    assert_eq!(
        targets,
        [
            format!("GET {API}/sobjects/Contact/describe/"),
            format!("POST {API}/jobs/query"),
        ]
    );

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn an_opt_out_value_that_is_not_a_boolean_fails_the_result_set_without_echoing_it() {
    let (server, _) = start().await;
    let ctx = context(&server, HOST_E, TOKEN_E);

    let error = mailing_list(&ctx, &deployment_export(), no_wait)
        .await
        .unwrap_err();
    match &error {
        MailingError::Results(SfError::Read(r)) => {
            assert_eq!(r.kind(), ReadErrorKind::Decode);
            let codec = r.codec().unwrap();
            assert_eq!(codec.get("record"), Some(1));
            assert_eq!(codec.get("field"), Some(7));
        }
        other => panic!("{other:?}"),
    }
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("unknown@example.test"), "{rendered}");
    assert!(!rendered.contains(TOKEN_E), "{rendered}");

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}
