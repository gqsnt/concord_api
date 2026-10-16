//! The canonical extended v3 Catalog vertical fixture (§22).
//!
//! Required features: `json` and `rate-limit`; execution against its fixed
//! HTTPS base additionally needs `default-tls`. Documentation owners:
//! `docs/spec.md`, `docs/generated_client.md` and `docs/overview.md`.
//! Proving tests: `tests/catalog_v3.rs` (public signatures of §22.3 and the
//! §22 semantics) and `tests/application.rs` (the §23 application examples
//! executed over TLS loopback).
//!
//! This fixture deliberately demonstrates root defaults, direct placement,
//! repeated values, an inherited fallback, exact body input, authentication
//! replacement, explicit rate schemas, head-only absence, byte exposure,
//! custom envelope traversal and streaming. Root `Json` defaults and the
//! regional routes form the current Catalog contract.
//!
//! Semantics worth reading twice:
//!
//! - `item` uses `when_present "locale" = locale_override`: an absent override
//!   keeps the inherited root `locale` occurrence, a present one replaces it.
//! - `inspect` uses `replace "Accept-Language" = language`: omission clears the
//!   scope's inherited header while both local query emissions are absent.
//! - The write policy keeps the custom label `"client.tenant"`; its `endpoint`
//!   dimension consumes the explicitly pinned `catalog.item.update` ID.
//! - `ApiVersion` intentionally renders `v2`: that is the provider's wire API
//!   version, independent of Concord's library major version.

// ANCHOR: catalog_v3_support
use std::fmt;
use std::num::NonZeroU32;

use concord::auth::{CredentialMaterial, SecretCredential};
use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{
    AtomError, Binary, BoundedAtomWriter, ByteStream, Json, RequestAtom, SecretString, api,
};
use serde::{Deserialize, Serialize};

pub use crate::pagination::envelope::{CursorStart, Envelope, EnvelopePages};

/// Tenant identifier; an alias keeps `String`'s role without token matching.
pub type TenantId = String;
/// Locale preference.
pub type Locale = String;
/// Region label (also a DNS label).
pub type Region = String;
/// Accept-Language value.
pub type Language = String;
/// List category.
pub type Category = String;
/// Repeated list tag.
pub type Tag = String;
/// Item identifier.
pub type ItemId = String;
/// Durable application command identifier used as an idempotency key.
pub type CommandId = String;

/// Default page size retained by list and feed requests.
pub const DEFAULT_PAGE_SIZE: NonZeroU32 = match NonZeroU32::new(25) {
    Some(n) => n,
    None => panic!("25 is nonzero"),
};

/// The provider's wire API version path segment.
///
/// It intentionally renders `v2`: the fixture provider's API version is
/// independent of the Concord library major version.
pub struct ApiVersion(&'static str);

impl Default for ApiVersion {
    fn default() -> Self {
        Self("v2")
    }
}

impl RequestAtom for ApiVersion {
    fn write_atom(&self, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(self.0)
    }
}

/// Login request body.
#[derive(Serialize)]
pub struct LoginInput {
    /// User name presented to the provider.
    pub user: String,
}

/// Update request body.
#[derive(Serialize)]
pub struct UpdateInput {
    /// New item title.
    pub title: String,
}

/// One list/feed entry.
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ItemSummary {
    /// Item identifier.
    pub id: String,
}

/// One item.
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct Item {
    /// Item identifier.
    pub id: String,
    /// Item title.
    pub title: String,
}

/// Session credential material returned by `authenticate`.
///
/// Debug never prints the material.
pub struct SessionToken(SecretString);

impl SessionToken {
    /// Wraps protected material, for example for a local
    /// `client.credentials().replace_session(..)`.
    #[must_use]
    pub fn new(secret: SecretString) -> Self {
        Self(secret)
    }
}

impl CredentialMaterial for SessionToken {}

impl SecretCredential for SessionToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(<redacted>)")
    }
}

impl<'de> Deserialize<'de> for SessionToken {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        SecretString::try_new(text)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}
// ANCHOR_END: catalog_v3_support

// ANCHOR: catalog_v3_declaration
api! {
    /// Typed, tenant-scoped Catalog provider client.
    client CatalogApi {
        base "https://catalog.example.test/root/";
        timeout 20s;
        request Json;
        response Json;

        values {
            /// The tenant is sent, but excluded from automatic observation.
            #[sensitive] query tenant: TenantId;
            query locale?: Locale;
            #[default] api_version: ApiVersion;
        }

        credentials {
            secret bootstrap_key;
            credential application_key = api_key(secret.bootstrap_key);
            credential session = endpoint authenticate;
        }

        rate_limits {
            policy tenant_reads(tenant) {
                id "catalog.tenant.read";
                key [origin, "tenant" = tenant];
                allow 120 per 1m;
                allow 5000 per 1h;
            }
        }

        subdomain ["api"];
        path "{api_version}";
        headers { "X-Client-Version" = "3"; }
        auth { header "X-Bootstrap-Key" = application_key; }

        /// Explicit login. Use store_as_session to request local installation.
        POST authenticate(body input: LoginInput) -> SessionToken {
            id "catalog.auth.login";
            path "auth/session";
            effect non_idempotent;
        }

        scope regional(
            region: Region,
            header("Accept-Language") language?: Language,
        ) {
            subdomain [region];
            path "regions/{region}";
            auth replace { bearer session challenge unauthorized; }
            rate_limit tenant_reads(tenant = client.tenant);

            /// One decoded page; select an explicit traversal terminal for more.
            GET item_summaries(
                query category?: Category,
                query offset: u64 = 0,
                query count: NonZeroU32 = DEFAULT_PAGE_SIZE,
                query("tag") each tags: Vec<Tag>,
            ) -> Vec<ItemSummary> {
                id "catalog.items.list";
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }

            /// A strict lookup. Its explicit byte terminal does not decode Json.
            GET item(id: ItemId, locale_override?: Locale) -> Item {
                id "catalog.item.get";
                path "items/{id}";
                query { when_present "locale" = locale_override; }
                expose accepted_bytes;
            }

            /// A head-only 404 is None; the body of that response is not polled.
            GET find_item(id: ItemId) -> Option<Item> {
                id "catalog.item.find";
                path "items/{id}";
                absent on 404;
            }

            /// The provider asserts idempotence for this command-bearing update.
            PUT update_item(
                id: ItemId,
                #[sensitive] header("Idempotency-Key") command_id: CommandId,
                body input: UpdateInput,
            ) -> Item {
                id "catalog.item.update";
                path "items/{id}";
                effect idempotent;
                rate_limit {
                    id "catalog.item.write";
                    key [origin, endpoint, "client.tenant" = client.tenant];
                    cost 5;
                    allow 30 per 1m;
                }
            }

            /// Cursor-envelope page; the controller owns extraction, not Json.
            GET feed(query after?: String, query count: NonZeroU32 = DEFAULT_PAGE_SIZE)
                -> Envelope<ItemSummary>
            {
                id "catalog.item.feed";
                path "feed";
                paginate via EnvelopePages {
                    init CursorStart { after: after };
                    cursor -> after;
                }
            }

            /// Returns a managed byte stream, not a completed export.
            GET export() -> ByteStream via Binary {
                id "catalog.item.export";
                path "export";
            }

            /// One caller-controlled optional is deliberately used three times.
            GET inspect(query language?: Language) -> Item {
                id "catalog.item.inspect";
                path "inspect";
                headers { replace "Accept-Language" = language; }
                query { "languageEcho" = language; }
            }
        }
    }
}
// ANCHOR_END: catalog_v3_declaration
