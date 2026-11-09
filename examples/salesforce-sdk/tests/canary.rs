//! Secret inputs, each with its own canary: the access token (`#[sensitive] token`,
//! src/api.rs:39; `AccessToken`, src/ids.rs:237, via `OrgContext::new`, src/context.rs:85) in
//! `Authorization`; a query locator (`#[sensitive]`, src/api.rs:88) in the `query/{locator}` path;
//! the response-only `nextRecordsUrl` locator (withheld by `QueryResponse`, src/model.rs:190),
//! which the traversal sends back; a Bulk result locator (`#[sensitive]`, src/api.rs:124) in
//! `?locator=`; a SOQL literal (withheld by `Literal`/`Soql` Debug, src/soql.rs:226,347, and by
//! `SfError`, src/error.rs:1) in `q` and in the Bulk job body. The token canary is also echoed
//! into response values the SDK withholds: Account fields (src/model.rs:150), error `message`
//! (src/model.rs:291), job `errorMessage` (src/bulk.rs:284, src/error.rs:633), `Sforce-Locator`
//! (src/bulk_query.rs:260), CSV row values (src/csv.rs:561), a refused returned link.
//! Seven scenarios for the SOQL read (generated op and `QueryPages`), the Bulk result-part read
//! and the Bulk query-job create (write). No wire exemptions. Not covered: native reqwest values,
//! `expose()`/`*_unredacted()`/`Soql::as_str` and locator `Display` (documented wire value). The
//! SOQL text is `#[sensitive]` in the declaration, so the generated query handle and its
//! `Prepared` are checked with every canary, the literal included.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

use concord_example_salesforce_sdk::bulk::{
    BulkDialect, ColumnDelimiter, CreateQueryJob, LineEnding,
};
use concord_example_salesforce_sdk::bulk_query::{
    BulkQueryJob, NextResultSet, PartComplete, ResultPartReader,
};
use concord_example_salesforce_sdk::csv::{CONTACT_EXPORT_FIELDS, ContactExportRow};
use concord_example_salesforce_sdk::ids::{
    AccessToken, ApiVersion, InstanceUrl, QueryJobId, QueryLocator, ResultLocator, SObjectName,
};
use concord_example_salesforce_sdk::model::Account;
use concord_example_salesforce_sdk::query::QueryPages;
use concord_example_salesforce_sdk::soql::{Condition, FieldPath, Literal, Op, Soql};
use concord_example_salesforce_sdk::{OrgContext, ResultLink, SalesforceApiConfig, SfError};
use concord_pagination::Resume;

const JOB: &str = "750A00000000001AAA";
const RECORD: &str = "001A00000000001AAA";

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn reply(case: usize, media: &str, success: String, malformed: String, echo: &str) -> Reply {
    let problem =
        format!(r#"[{{"errorCode":"INVALID_FIELD","message":"{echo}","fields":["Name"]}}]"#);
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::json(400, &problem),
        2 => Reply::json(503, &problem),
        3 => Reply::text(200, media, malformed),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
        .header("sforce-limit-info", "api-usage=7/15000")
}

/// The org by resource: SOQL batches, the Bulk job document, CSV result sets.
fn respond(case: usize, r: &Recorded, echo: &str, next: &str) -> Reply {
    let path = r.path();
    if path.ends_with("/results") {
        let header = CONTACT_EXPORT_FIELDS.map(|f| format!("\"{f}\"")).join(",");
        let row = format!(r#""003A00000000001AAA","{echo}","{echo}","{echo}","","{echo}","false""#);
        let success = format!("{header}\n{row}\n");
        let malformed = format!("{header}\n\"003A00000000001AAA\",\"{echo}\n");
        let reply = reply(case, "text/csv; charset=UTF-8", success, malformed, echo);
        return reply
            .header("sforce-locator", echo)
            .header("sforce-numberofrecords", "1");
    }
    let record = format!(
        r#"{{"attributes":{{"type":"Account","url":"/services/data/v68.0/sobjects/Account/{RECORD}"}},"Id":"{RECORD}","Name":"{echo}","Description":"{echo}"}}"#
    );
    let success = if path.ends_with("/jobs/query") {
        format!(
            r#"{{"id":"{JOB}","operation":"query","object":"Account","state":"UploadComplete","apiVersion":68.0,"contentType":"CSV","columnDelimiter":"COMMA","lineEnding":"LF","errorMessage":"{echo}"}}"#
        )
    } else if path.ends_with("/query") {
        format!(r#"{{"totalSize":2,"done":false,"nextRecordsUrl":"{next}","records":[{record}]}}"#)
    } else {
        format!(r#"{{"totalSize":2,"done":true,"records":[{record}]}}"#)
    };
    reply(
        case,
        "application/json",
        success,
        format!("{{invalid {echo}"),
        echo,
    )
}

fn concord_error<D: 'static>(c: &mut Canaries, e: &concord::Error<D>) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

fn error(c: &mut Canaries, e: &SfError) {
    c.assert_debug_absent("SfError", e);
    c.assert_error_absent("SfError", e);
    c.assert_debug_absent("SfError API usage", &e.api_usage());
    match e {
        SfError::Concord(e) => concord_error(c, e),
        SfError::Api(a) => {
            c.assert_debug_absent("ApiFailure", a);
            c.assert_display_absent("ApiFailure", a);
            c.assert_debug_absent("error documents", a.errors());
            c.assert_debug_absent("error body state", &a.body_state());
        }
        SfError::Open(o) => c.assert_error_absent("OpenError", o.as_ref()),
        SfError::Read(r) => c.assert_error_absent("ReadError", r.as_ref()),
        SfError::Protocol(p) => c.assert_error_absent("ProtocolError", p.as_ref()),
        SfError::Link(l) => c.assert_error_absent("LinkError", l),
        SfError::Incomplete(i) => c.assert_error_absent("IncompletePart", i.as_ref()),
        SfError::Job(j) => {
            c.assert_debug_absent("JobFailure", j);
            c.assert_display_absent("JobFailure", j);
        }
        SfError::Csv(e) => {
            c.assert_debug_absent("CsvError", e);
            c.assert_display_absent("CsvError", e);
        }
        _ => {}
    }
}

/// Opens one result part, reads every row and validates its end.
async fn read_part(
    c: &mut Canaries,
    reader: &ResultPartReader<ContactExportRow>,
    link: &ResultLink,
) -> Result<(PartComplete, NextResultSet), SfError> {
    let mut rows = reader.open(link).await?;
    c.assert_debug_absent("result part rows", &rows);
    c.assert_debug_absent("next result set", rows.next_set());
    c.assert_debug_absent("CSV records", rows.records());
    let next = rows.next_set().clone();
    while let Some(row) = rows.next().await {
        c.assert_debug_absent("result row Result", &row);
        if let Err(e) = row {
            c.assert_debug_absent("failed result part rows", &rows);
            error(c, &rows.finish().unwrap_err());
            return Err(e);
        }
    }
    c.assert_debug_absent("read result part rows", &rows);
    let complete = rows.finish()?;
    c.assert_debug_absent("part completion", &complete);
    Ok((complete, next))
}

fn context(origin: &str, token: &AccessToken) -> OrgContext {
    OrgContext::new(
        http(),
        InstanceUrl::parse(origin).unwrap(),
        ApiVersion::PINNED,
        token.clone(),
    )
    .unwrap()
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("salesforce");
    let token = c.issue();
    let query_locator = c.issue();
    let next_locator = c.issue();
    let result_locator = c.issue();
    let literal = c.issue();

    let access = AccessToken::new(&token).unwrap();
    c.assert_debug_absent("AccessToken", &access);
    let config = SalesforceApiConfig {
        instance: InstanceUrl::parse("https://canary.my.salesforce.test").unwrap(),
        token: token.clone(),
        version: concord_example_salesforce_sdk::ids::SupportedVersion::new(ApiVersion::PINNED)
            .unwrap(),
    };
    c.assert_debug_absent("generated config", &config);
    let name = FieldPath::parse("Name").unwrap();
    let condition = Condition::new(name.clone(), Op::Eq, Literal::text(&literal));
    c.assert_debug_absent("SOQL condition", &condition);
    c.assert_debug_absent("SOQL LIKE literal", &Literal::like_contains(&literal));
    let builder = Soql::select(
        SObjectName::new("Account").unwrap(),
        [FieldPath::parse("Id").unwrap(), name],
    )
    .filter(condition);
    c.assert_debug_absent("SOQL builder", &builder);
    let soql = builder.build().unwrap();
    c.assert_debug_absent("SOQL statement", &soql);
    let dialect = BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf);
    c.assert_debug_absent("Bulk job body", &CreateQueryJob::new(&soql, dialect));
    let locator = QueryLocator::new(&query_locator).unwrap();
    c.assert_debug_absent("query locator", &locator);
    let link = ResultLink {
        locator: Some(ResultLocator::new(&result_locator).unwrap()),
        max_records: None,
    };
    c.assert_debug_absent("result link", &link);
    let columns: Vec<FieldPath> = CONTACT_EXPORT_FIELDS
        .iter()
        .map(|f| FieldPath::parse(f).unwrap())
        .collect();

    for case in 0..7 {
        let echo = token.clone();
        let next = format!("/services/data/v68.0/query/{next_locator}");
        let server = TestServer::start(move |r: &Recorded| respond(case, r, &echo, &next)).await;
        let ctx = context(&if case == 6 { refused() } else { server.url() }, &access);
        let api = ctx.api();
        c.assert_debug_absent("OrgContext", &ctx);
        c.assert_debug_absent("generated client", api);
        c.assert_debug_absent("query scope", &api.query());
        c.assert_debug_absent("bulk query scope", &api.bulk_query());
        let job_scope = api.bulk_query().job(QueryJobId::new(JOB).unwrap());
        c.assert_debug_absent("bulk job scope", &job_scope);

        // Read: SOQL as an ordinary generated operation, then the SDK's error conversion.
        let op = api.query().accounts(soql.clone());
        c.assert_debug_absent("SOQL endpoint", &op);
        c.assert_debug_absent("SOQL Prepared", &op.clone().prepare().unwrap());
        let result = op.send().await;
        c.assert_debug_absent("SOQL Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(batch) => {
                c.assert_debug_absent("query batch", &batch);
                c.assert_debug_absent("Account", &batch.records[0]);
                c.assert_debug_absent("record attributes", &batch.records[0].attributes);
                assert_eq!(batch.records[0].name, token);
            }
            Err(e) => {
                concord_error(&mut c, &e);
                let e = SfError::from_concord(e).await;
                error(&mut c, &e);
                if case == 1 {
                    let api_failure = e.api().unwrap();
                    assert_eq!(api_failure.status().as_u16(), 400);
                    assert_eq!(
                        api_failure.operation(),
                        Some("SalesforceApi.query.accounts")
                    );
                    assert_eq!(api_failure.errors()[0].message_unredacted(), token);
                    assert!(e.has_error_code("INVALID_FIELD"));
                    assert_eq!(
                        e.to_string(),
                        "SalesforceApi.query.accounts: Salesforce answered status 400 [INVALID_FIELD]"
                    );
                }
            }
        }
        let more = api.query().more_accounts(locator.clone());
        c.assert_debug_absent("query-more endpoint", &more);
        receive(&mut c, more.prepare().unwrap()).await;

        // Read: the SOQL traversal follows the returned locator.
        let mut pages = QueryPages::<Account>::new(&ctx, soql.clone()).into_pages(Resume::Start);
        c.assert_debug_absent("query traversal", &pages);
        let page = pages.next_page().await;
        c.assert_debug_absent("query page Result", &page);
        assert_eq!(page.is_ok(), case == 0);
        match page {
            Ok(page) => {
                let page = page.unwrap();
                c.assert_debug_absent("query page", &page);
                assert_eq!(page.next.as_ref().unwrap().expose(), next_locator);
                let last = pages.next_page().await.unwrap().unwrap();
                c.assert_debug_absent("last query page", &last);
                assert!(pages.next_page().await.unwrap().is_none());
            }
            Err(e) => {
                c.assert_error_absent("PagesError", &e);
                c.assert_debug_absent("PagesError resume", &e.resume());
                error(&mut c, e.provider_error().unwrap());
            }
        }
        c.assert_debug_absent("query checkpoint", &pages.checkpoint());
        c.assert_debug_absent("query progress", &pages.progress());
        c.assert_debug_absent("query report", &pages.finish());

        // Write: create a Bulk query job (the SOQL travels in the JSON body).
        let create = api.bulk_query().create(CreateQueryJob::new(&soql, dialect));
        c.assert_debug_absent("create endpoint", &create);
        c.assert_debug_absent("create Prepared", &create.prepare().unwrap());
        let created = BulkQueryJob::create(&ctx, &soql, dialect).await;
        c.assert_debug_absent("create Result", &created);
        assert_eq!(created.is_ok(), case == 0);
        match created {
            Ok((job, info)) => {
                c.assert_debug_absent("BulkQueryJob", &job);
                c.assert_debug_absent("QueryJobInfo", &info);
                assert_eq!(info.error_message_unredacted(), Some(token.as_str()));
            }
            Err(e) => error(&mut c, &e),
        }

        // Read: one Bulk result part (CSV) at a result locator.
        let job = BulkQueryJob::attach(&ctx, QueryJobId::new(JOB).unwrap(), dialect);
        let reader = ResultPartReader::<ContactExportRow>::new(&job, &columns).unwrap();
        c.assert_debug_absent("result part reader", &reader);
        let results = job_scope
            .results()
            .set_locator(ResultLocator::new(&result_locator).unwrap());
        c.assert_debug_absent("results endpoint", &results);
        c.assert_debug_absent("results Prepared", &results.prepare().unwrap());
        let part = read_part(&mut c, &reader, &link).await;
        c.assert_debug_absent("result part Result", &part);
        assert_eq!(part.is_ok(), case == 0);
        match part {
            Ok((complete, next)) => {
                assert_eq!(complete.records, 1);
                assert_eq!(next, NextResultSet::At(ResultLocator::new(&token).unwrap()));
            }
            Err(e) => error(&mut c, &e),
        }

        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }

    // A returned link refused before use, and a failed job: both withhold what they carry.
    let echo = token.clone();
    let foreign = format!("https://evil.example/services/data/v68.0/query/{next_locator}");
    let server = TestServer::start(move |r: &Recorded| {
        if r.path().contains("/jobs/query/") {
            Reply::json(
                200,
                format!(
                    r#"{{"id":"{JOB}","operation":"query","state":"Failed","apiVersion":68.0,"errorMessage":"{echo}"}}"#
                ),
            )
        } else {
            respond(0, r, &echo, &foreign)
        }
    })
    .await;
    let ctx = context(&server.url(), &access);
    let mut pages = QueryPages::<Account>::new(&ctx, soql.clone()).into_pages(Resume::Start);
    let e = pages.next_page().await.unwrap_err();
    c.assert_error_absent("PagesError (link)", &e);
    let e = e.provider_error().unwrap();
    error(&mut c, e);
    assert!(matches!(e, SfError::Link(_)));
    let job = BulkQueryJob::attach(&ctx, QueryJobId::new(JOB).unwrap(), dialect);
    let e = job
        .wait_complete(NonZeroUsize::new(1).unwrap(), |_| async {})
        .await
        .unwrap_err();
    error(&mut c, &e);
    let SfError::Job(failure) = &e else {
        panic!("expected a job failure: {e}");
    };
    assert_eq!(failure.error_message_unredacted(), Some(token.as_str()));
    assert_eq!(e.to_string(), format!("Bulk job {JOB} ended Failed"));
    server.shutdown().await;

    // Local refusals never echo the refused input.
    let e = ctx
        .query_link(&format!(
            "https://evil.example/services/data/v68.0/query/{next_locator}"
        ))
        .unwrap_err();
    c.assert_error_absent("local LinkError", &e);
    let e = AccessToken::new(format!("{token} x")).unwrap_err();
    c.assert_error_absent("refused token", &e);
    let e = QueryLocator::new(format!("{query_locator}/x")).unwrap_err();
    c.assert_error_absent("refused query locator", &e);
    let e = ResultLocator::new(format!("{result_locator}!")).unwrap_err();
    c.assert_error_absent("refused result locator", &e);

    assert!(c.checked() >= 300);
    println!(
        "salesforce: {} renderings; positive control passed",
        c.checked()
    );
}
