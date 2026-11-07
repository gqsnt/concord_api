//! The declaration: one org's REST and Bulk API 2.0 resources under
//! `{instance}/services/data/v{version}/`.
//!
//! Every endpoint is an ordinary generated operation (setters, `prepare`, `send`, `receive`,
//! `into_parts`). The typed wrappers of this crate construct these operations; callers can use
//! them directly too, from [`crate::OrgContext::api`].
//!
//! A declared endpoint has one output type, so the query and record endpoints are declared once
//! per typed record of the selected schema (`Account`, `Contact`) plus once for
//! [`DynamicRecord`]; see `FRICTION.md` (F1).
//!
//! CSV endpoints (`results`, `successful_results`, ...) are declared `-> stream` without an item:
//! their dialect is chosen per job at run time, which a static format type cannot express, so
//! their `send()` returns the unread 2xx body ([`concord::Incoming`]; a non-2xx status is a
//! `Status` error) with the endpoint's identity, and the SDK opens it with a runtime
//! [`concord::csv::CsvReader`] built from the job's dialect ([`crate::csv`]). Every reader error
//! then names the declared endpoint. Nothing is drained or buffered.

use concord::Binary;
use concord::Json;
use concord::bytes::Bytes;
use concord::reqwest::header::{ACCEPT, AUTHORIZATION};

use crate::bulk::{
    CreateIngestJob, CreateQueryJob, IngestJobInfo, JobStateChange, QueryJobInfo,
    ResultPagesResponse,
};
use crate::ids::{
    ApiVersion, BatchSize, FieldList, IngestJobId, MaxRecords, QueryJobId, QueryLocator, RecordId,
    ResultLocator, SObjectName,
};
use crate::model::{Account, Contact, DynamicRecord, QueryResponse, SObjectDescribe};
use crate::soql::Soql;

concord::api! {
    /// One Salesforce org at one pinned API version: REST (describe, records, SOQL) and Bulk API
    /// 2.0 (query and ingest jobs). The base is replaced by the org's instance URL
    /// ([`crate::OrgContext`] does it).
    pub client SalesforceApi(#[sensitive] token: String, version: ApiVersion) {
        base "https://instance.salesforce.invalid";
        path "services/data/v{version}";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;

        /// `sobjects/Account`: the typed Account resource.
        scope accounts {
            path "sobjects/Account";

            /// sObject Describe (`GET sobjects/Account/describe/`).
            GET describe() -> SObjectDescribe { path "describe/"; }
            /// sObject Rows (`GET sobjects/Account/{id}?fields=..`).
            GET get(id: RecordId, query fields: FieldList) -> Account { path "{id}"; }
        }

        /// `sobjects/Contact`: the typed Contact resource.
        scope contacts {
            path "sobjects/Contact";

            /// sObject Describe (`GET sobjects/Contact/describe/`).
            GET describe() -> SObjectDescribe { path "describe/"; }
            /// sObject Rows (`GET sobjects/Contact/{id}?fields=..`).
            GET get(id: RecordId, query fields: FieldList) -> Contact { path "{id}"; }
        }

        /// `sobjects/{name}`: any object, every field dynamic.
        scope sobject(name: SObjectName) {
            path "sobjects/{name}";

            /// sObject Describe.
            GET describe() -> SObjectDescribe { path "describe/"; }
            /// sObject Rows; all fields when `fields` is absent.
            GET get(id: RecordId, query fields?: FieldList) -> DynamicRecord { path "{id}"; }
        }

        /// `query`: SOQL (`GET query?q=..`) and query more (`GET query/{locator}`).
        /// `Sforce-Query-Options: batchSize=N` when a batch size is set.
        scope query(batch_size?: BatchSize) {
            path "query";
            headers { "sforce-query-options" = "batchSize={batch_size}"; }

            /// Runs a SOQL statement returning Accounts.
            GET accounts(#[sensitive] query q: Soql) -> QueryResponse<Account>;
            /// Runs a SOQL statement returning Contacts.
            GET contacts(#[sensitive] query q: Soql) -> QueryResponse<Contact>;
            /// Runs a SOQL statement returning dynamic records.
            GET dynamic(#[sensitive] query q: Soql) -> QueryResponse<DynamicRecord>;
            /// The next batch of an Account query (`nextRecordsUrl`'s locator).
            GET more_accounts(#[sensitive] locator: QueryLocator) -> QueryResponse<Account> {
                path "{locator}";
            }
            /// The next batch of a Contact query.
            GET more_contacts(#[sensitive] locator: QueryLocator) -> QueryResponse<Contact> {
                path "{locator}";
            }
            /// The next batch of a dynamic query.
            GET more_dynamic(#[sensitive] locator: QueryLocator) -> QueryResponse<DynamicRecord> {
                path "{locator}";
            }
        }

        /// `jobs/query`: Bulk API 2.0 query jobs.
        scope bulk_query {
            path "jobs/query";

            /// Creates a query job (`POST jobs/query`).
            POST create(body job: CreateQueryJob) -> QueryJobInfo;

            /// One query job.
            scope job(id: QueryJobId) {
                path "{id}";

                /// Job information (`GET jobs/query/{id}`).
                GET info() -> QueryJobInfo;
                /// Aborts the job (`PATCH jobs/query/{id}` with `{"state": "Aborted"}`).
                PATCH abort(body change: JobStateChange) -> QueryJobInfo;
                /// The parallel result manifest (`GET jobs/query/{id}/resultPages`); the next
                /// manifest is addressed by the `locator` of the returned next link.
                GET result_pages(#[sensitive] query locator?: ResultLocator) -> ResultPagesResponse {
                    path "resultPages";
                }
                /// One set of CSV results (`GET jobs/query/{id}/results`): the unread body, read
                /// with the job's CSV dialect by [`crate::bulk_query::ResultPartReader`].
                GET results(
                    #[sensitive] query locator?: ResultLocator,
                    query("maxRecords") max_records?: MaxRecords,
                ) -> stream {
                    path "results";
                    headers { ACCEPT = "text/csv"; }
                }
            }
        }

        /// `jobs/ingest`: Bulk API 2.0 ingest jobs.
        scope ingest {
            path "jobs/ingest";

            /// Creates an ingest job (`POST jobs/ingest`).
            POST create(body job: CreateIngestJob) -> IngestJobInfo;

            /// One ingest job.
            scope job(id: IngestJobId) {
                path "{id}";

                /// Job information (`GET jobs/ingest/{id}`).
                GET info() -> IngestJobInfo;
                /// Changes the state: `UploadComplete` closes the upload, `Aborted` aborts
                /// (`PATCH jobs/ingest/{id}`).
                PATCH set_state(body change: JobStateChange) -> IngestJobInfo;
                /// Uploads the job's CSV data (`PUT jobs/ingest/{id}/batches`,
                /// `Content-Type: text/csv`), as one complete body of known length.
                PUT upload(body csv: Bytes) {
                    path "batches";
                    request Binary "text/csv";
                }
                /// Successfully processed records (CSV with `sf__Id`, `sf__Created`).
                GET successful_results() -> stream {
                    path "successfulResults/";
                    headers { ACCEPT = "text/csv"; }
                }
                /// Failed records (CSV with `sf__Id`, `sf__Error`).
                GET failed_results() -> stream {
                    path "failedResults/";
                    headers { ACCEPT = "text/csv"; }
                }
                /// Unprocessed records (CSV of the uploaded fields).
                GET unprocessed_records() -> stream {
                    path "unprocessedrecords/";
                    headers { ACCEPT = "text/csv"; }
                }
            }
        }
    }
}
