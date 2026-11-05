//! External caller of the Salesforce SDK slice: the two required workflows as library functions
//! taking a configured [`OrgContext`](concord_example_salesforce_sdk::OrgContext) and explicit
//! inputs, returning typed reports.
//!
//! - [`sf_a::crm_extraction`] — SF-A schema-aware CRM extraction.
//! - [`sf_b::bulk_csv_export`] / [`sf_b::continue_export`] — SF-B streamed Bulk CSV export with
//!   discovered parallel result parts, staged shards and restart.
//! - [`ingest::ingest_round_trip`] — the SF-B offline ingest sub-scenario (public encoder, bounded
//!   spool, create/upload/close/poll, result files).
//!
//! The binaries in `src/bin/` run the same functions against a live org when credentials (and,
//! for job creation or writes, an explicit sandbox opt-in) are present; otherwise they print
//! `NOT_RUN`.

pub mod ingest;
pub mod sf_a;
pub mod sf_b;
pub mod sink;
