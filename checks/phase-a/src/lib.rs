//! Declarations shared by the Phase A wire tests: a small reports service with finite, metadata,
//! streaming and runtime-upload endpoints, written the way an SDK author would.

use concord::csv::{Csv, HasHeader, Semicolon};
use concord::jsonl::{JsonLines, JsonStream};
use concord::stream::Upload;
use concord::{Encoded, EncodedBody, Json};
use serde::{Deserialize, Serialize};

/// One report row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Identifier.
    pub id: u64,
    /// Amount in minor units.
    pub amount: u64,
}

/// One watch-style event.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Event {
    /// `ADDED`, `MODIFIED`, ...
    #[serde(rename = "type")]
    pub kind: String,
    /// The object's name.
    pub name: String,
}

/// The fixed report dialect.
pub type ReportCsv = Csv<HasHeader, Semicolon>;

concord::api! {
    /// Reports service.
    pub client Reports(#[sensitive] header("authorization") token: String) {
        base "http://reports.invalid/v1";
        format Json;

        /// A finite JSON document.
        GET summary(id: String) -> Row { path "reports/{id}/summary"; }
        /// The report's rows as they arrive.
        GET rows(id: String) -> stream Row {
            path "reports/{id}";
            response ReportCsv;
        }
        /// Change events (consecutive JSON values).
        GET events(#[sensitive] query("token") resume?: String) -> stream Event {
            path "events";
            response JsonStream;
        }
        /// Audit lines (JSON Lines).
        GET audit() -> stream Row { path "audit"; response JsonLines; }
        /// A job export whose dialect is known only at run time.
        GET export(id: String) -> stream { path "jobs/{id}/export"; }
        /// Uploads a job export encoded at run time.
        POST upload(id: String, body export: Upload) -> Row {
            path "jobs/{id}/export";
            request Encoded;
        }
        /// The same, with a declared media type the runtime one must equal.
        POST upload_declared(id: String, body export: Upload) -> Row {
            path "jobs/{id}/export";
            request Encoded "text/csv; header=present";
        }
        /// The same, with a `content-type` header contribution.
        POST upload_header(id: String, ct: String, body export: Upload) -> Row {
            path "jobs/{id}/export";
            request Encoded;
            headers { "content-type" = "{ct}"; }
        }
        /// The same, removing an inherited `content-type`.
        scope legacy {
            headers { "content-type" = "application/octet-stream"; }
            POST upload(id: String, body export: Upload) -> Row {
                path "jobs/{id}/export";
                request Encoded;
                headers { remove "content-type"; }
            }
        }
        /// An optional pre-encoded body.
        POST attach(id: String, body attachment?: EncodedBody) -> Row {
            path "jobs/{id}/attachment";
            request Encoded;
        }
    }
}
