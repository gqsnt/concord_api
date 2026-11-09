//! A small service SDK written with `concord::api!`: two independent services, Catalog and
//! Ledger, each with its own base, credentials, formats, pagination and account representation.
//!
//! It is the shared declaration of the SDK examples:
//!
//! - `examples/app` converts application domain values into these operations, names
//!   application workflows and walks both services' pages;
//! - `examples/extension` adds a Catalog operation this SDK does not declare, from another crate,
//!   through the public [`context`] boundary;
//! - the generated-input experiment (`v4/dx-4.3/experiments/generated-inputs`) compiles the same
//!   declaration with an experimental macro.
//!
//! Wire representations are the services' own: Catalog writes an account as a decimal number and
//! a time as whole-second RFC 3339 UTC; Ledger writes an account as `acct-` plus lowercase hex
//! and a time as nonnegative milliseconds. This crate takes them as already-formatted text: which
//! domain types an application converts from is the application's business.

pub mod context;

use concord::bytes::Bytes;
use concord::reqwest;
use concord::{CodecError, Encode, Json, Text};
use serde::{Deserialize, Serialize};

/// A Catalog item.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Item {
    /// Identifier, such as `item-001`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Distinct tags.
    pub tags: Vec<String>,
}

/// One page of `GET tenants/{tenant}/items`.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemPage {
    /// Items on this page.
    pub items: Vec<Item>,
    /// Opaque cursor of the next page; `None` on the last page.
    pub next_cursor: Option<String>,
}

/// Body of `POST tenants/{tenant}/items`.
#[derive(Debug, Clone, Serialize)]
pub struct NewItem {
    /// Display name; the service refuses an empty one (`422`).
    pub name: String,
    /// Tags.
    pub tags: Vec<String>,
}

/// One account activity event.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Activity {
    /// Identifier, such as `event-001`.
    pub id: String,
    /// The account, in Catalog's decimal representation.
    pub account_id: String,
    /// When, whole-second RFC 3339 UTC.
    pub at: String,
}

/// `GET accounts/{account}/activity`.
#[derive(Debug, Clone, Deserialize)]
pub struct ActivityPage {
    /// Events, oldest first.
    pub events: Vec<Activity>,
}

/// What `PUT tenants/{tenant}/items/{id}/attachment` received.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadReceipt {
    /// Body length as received.
    pub bytes_received: u64,
    /// The `x-client-note` request header, if one was sent.
    pub client_note: Option<String>,
}

/// One Ledger entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Entry {
    /// Identifier, such as `entry-001`.
    pub id: String,
    /// The account, in Ledger's `acct-` representation.
    pub account: String,
    /// When, milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// Signed amount in minor units.
    pub amount_minor: i64,
}

/// Ledger's paging envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct Paging {
    /// Opaque token of the next page; `None` on the last page.
    pub next: Option<String>,
}

/// One page of `GET entries`.
#[derive(Debug, Clone, Deserialize)]
pub struct EntryPage {
    /// Entries on this page, oldest first.
    pub data: Vec<Entry>,
    /// The next page, if any.
    pub paging: Paging,
}

/// A finite attachment, moved into one request body. Deliberately **not** `Clone`: an upload
/// consumes it, and sending again needs a new `Attachment`.
pub struct Attachment {
    bytes: Bytes,
}

impl Attachment {
    /// An attachment of these bytes.
    pub fn new(bytes: impl Into<Bytes>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    /// Its length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether it has no bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// `Encode::CONTENT_TYPE` sends `content-type: application/octet-stream` with the body.
///
/// `application/octet-stream` for an [`Attachment`]: moves the bytes into the body.
pub struct Octets;

impl Encode<Attachment> for Octets {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Attachment) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.bytes))
    }
}

concord::api! {
    /// The Catalog service: items per tenant, and account activity.
    pub client Catalog(
        /// The API key, sent as `x-api-key`.
        #[sensitive] header("x-api-key") key: String,
        /// The region context, sent as `x-region`. Absent: the service's default region.
        header("x-region") region?: String,
    ) {
        base "https://catalog.example.test/catalog/v1";
        headers { "x-client" = "concord-example-sdk"; }
        format Json;

        /// Service health, as text.
        GET health() -> String { path "health"; response Text; }

        /// Activity of one account. `account` is Catalog's decimal account id; `since` a
        /// whole-second RFC 3339 UTC timestamp (absent: no lower bound).
        GET activity(account: String, query since?: String) -> ActivityPage {
            path "accounts/{account}/activity";
        }

        /// One tenant. The tenant is path data: any text, percent-encoded as one segment.
        scope tenant(tenant: String) {
            path "tenants/{tenant}";

            /// Listing defaults shared by every list request created from this handle.
            scope listing(
                /// Tags every list request sends unless the call replaces them (`tags`).
                query("tag") each default_tags: Vec<String> = Vec::new(),
            ) {
                /// Lists items, `limit` per page.
                GET list(
                    /// A name filter.
                    query q?: String,
                    /// Absent: the listing's `default_tags`. Present: exactly these tags (an empty
                    /// list sends no `tag` at all).
                    tags?: Vec<String>,
                    /// Page size, 1 through 100.
                    query limit: u32 = 20,
                    /// Absent: the client's `x-region`. `Some(None)`: no `x-region`.
                    /// `Some(Some(r))`: region `r` for this call.
                    region?: Option<String>,
                    /// The `next_cursor` of the previous page.
                    query cursor?: String,
                ) -> ItemPage {
                    path "items";
                    query { override "tag" = each tags else inherit; }
                    headers { override "x-region" = each region else inherit; }
                }
            }

            /// One item; an unknown id is a `404` status error.
            GET item(id: String) -> Item { path "items/{id}"; }

            /// Creates an item; an empty name is a `422` status error.
            POST create(body item: NewItem) -> Item { path "items"; }

            /// Deletes an item.
            DELETE delete(id: String) { path "items/{id}"; }

            /// Replaces an item's attachment. The attachment is moved into the body.
            PUT attach(id: String, body file: Attachment) -> UploadReceipt {
                path "items/{id}/attachment";
                request Octets;
            }
        }
    }
}

concord::api! {
    /// The Ledger service: account entries.
    pub client Ledger(
        /// The full `Authorization` value, such as `Bearer ...`.
        #[sensitive] header("authorization") token: String,
    ) {
        base "https://ledger.example.test/ledger/v1";
        response Json;

        /// Entries of one account, oldest first. `account` is Ledger's `acct-` representation;
        /// `from_ms` nonnegative milliseconds since the Unix epoch (absent: no lower bound).
        GET entries(
            query account: String,
            query from_ms?: String,
            /// Page size, 1 through 50.
            query page_size: u32 = 10,
            /// The `paging.next` of the previous page.
            query page_token?: String,
        ) -> EntryPage {
            path "entries";
        }
    }
}
