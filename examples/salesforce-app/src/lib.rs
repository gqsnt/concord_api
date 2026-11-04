//! External caller of the Salesforce SDK slice: the two required workflows as library functions
//! taking a configured [`OrgContext`](concord_example_salesforce_sdk::OrgContext) and explicit
//! inputs, returning typed reports.
//!
//! - [`sf_a::crm_extraction`] — SF-A schema-aware CRM extraction.
//!
//! The binaries in `src/bin/` run the same functions against a live org when credentials (and,
//! for job creation or writes, an explicit sandbox opt-in) are present; otherwise they print
//! `NOT_RUN`.

pub mod sf_a;
