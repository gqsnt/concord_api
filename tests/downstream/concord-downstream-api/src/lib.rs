#![forbid(unsafe_code)]
//! A downstream crate that declares a Concord client with `api!`.
//!
//! This package and `concord-downstream-app` form the two-crate acceptance
//! package of V3-L14/V3-L15 (part of V3-O06): the macro expands **here**, in a
//! crate that is neither the facade nor the core, and a second crate consumes
//! the generated named requests through their public surface only. No
//! stand-in type defined beside a library seal is involved.
//!
//! The declaration is deliberately small but still introduces two distinct
//! ordinary endpoint request types, a paginated request with retained
//! defaults, a scope with a required positional input, optional and
//! initialized value controls, and a root value.
//!
//! [`MeteredApi`] adds the rate-limit half of the package (V3-R13): a
//! `rate_limits` registry with a window set and a named policy whose formal
//! key dimensions use a `via` adapter ([`LowercaseRegion`]) and a
//! constant-bearing custom [`RateKeyPart`] ([`ShardKey`]), plus inline
//! attachments with a primitive (`u32`) dimension and a custom
//! [`RateKeyEncoder`] adapter ([`AccountKey`]) for an opaque type. The key
//! types, adapters and the expansion all live in this crate; the consuming
//! crate observes the produced key bytes and encoding IDs.

use std::num::NonZeroU32;

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::rate_limit::{CanonicalKeyWriter, RateKeyEncoder, RateKeyPart, RateLimitError};
use concord::{Json, api};
use serde::Deserialize;

/// One widget.
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct Widget {
    /// Widget identifier.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// One gadget.
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct Gadget {
    /// Gadget identifier.
    pub id: String,
}

api! {
    /// Downstream-declared inventory client.
    client InventoryApi {
        base "https://localhost/v1/";
        request Json;
        response Json;

        values {
            query tenant: String;
            header("X-Trace") trace?: String;
        }

        scope shop(shop: String) {
            path "shops/{shop}";

            /// One widget; `view` is an optional caller-controlled query.
            GET widget(id: String, query view?: String) -> Widget {
                id "inventory.widget.get";
                path "widgets/{id}";
            }

            /// One gadget: a second, distinct generated request type.
            GET gadget(id: String) -> Gadget {
                id "inventory.gadget.get";
                path "gadgets/{id}";
            }

            /// One decoded widget page; traversal terminals are explicit.
            GET widgets(
                query offset: u64 = 0,
                query limit: NonZeroU32 = NonZeroU32::MIN,
            ) -> Vec<Widget> {
                id "inventory.widgets.list";
                path "widgets";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: limit };
                    cursor -> offset;
                }
            }
        }
    }
}

/// A shard number with its own constant-bearing canonical key contract.
///
/// `RateKeyPart` carries an associated `ENCODING_ID`, so it is not
/// dyn-compatible: the generated key path must encode it through typed code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShardKey(pub u16);

impl ShardKey {
    /// The canonical type tag written into the key bytes.
    pub const TAG: &'static str = "downstream.shard/v1";
}

impl RateKeyPart for ShardKey {
    const ENCODING_ID: &'static str = "downstream.shard/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes(Self::TAG, &self.0.to_be_bytes())
    }
}

/// The formal `via` adapter of the `metered` policy: canonical lowercase
/// text under the built-in `utf8` tag, with its own encoding contract.
pub struct LowercaseRegion;

impl RateKeyEncoder<String> for LowercaseRegion {
    const ENCODING_ID: &'static str = "downstream.lowercase/1";

    fn write_canonical(value: &String, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("utf8", value.to_ascii_lowercase().as_bytes())
    }
}

/// An opaque account identifier with no `Display`, `RequestAtom` or
/// `RateKeyPart` implementation: its only role is a rate dimension through
/// the explicit [`AccountKey`] adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountId(u64);

impl AccountId {
    /// Wraps a numeric account identifier.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

/// The custom canonical adapter for [`AccountId`].
pub struct AccountKey;

impl AccountKey {
    /// The canonical type tag written into the key bytes.
    pub const TAG: &'static str = "downstream.account/v1";
}

impl RateKeyEncoder<AccountId> for AccountKey {
    const ENCODING_ID: &'static str = "downstream.account/1";

    fn write_canonical(
        value: &AccountId,
        out: &mut CanonicalKeyWriter,
    ) -> Result<(), RateLimitError> {
        out.write_tagged_bytes(Self::TAG, &value.0.to_be_bytes())
    }
}

api! {
    /// Downstream-declared metered client (V3-R13): a registry with a window
    /// set and a named policy with formal key dimensions, plus bound inline
    /// attachments at a scope and an endpoint.
    client MeteredApi {
        base "https://localhost/metered/";
        response Json;

        values {
            region: String;
        }

        rate_limits {
            windows standard { allow 100 per 1m; allow 1000 per 1h; }

            policy metered(region, shard) {
                id "downstream.metered";
                key [origin, "region" = region via LowercaseRegion, "shard" = shard];
                using standard;
            }
        }

        scope accounts(account: AccountId) {
            rate_limit {
                id "downstream.account";
                key ["account" = account via AccountKey];
                allow 10 per 1s;
            }

            /// Daily usage of one account shard.
            GET usage(shard: ShardKey, day: u32) -> Widget {
                id "downstream.usage.get";
                path "usage/{day}";
                rate_limit metered(region = client.region, shard = shard);
                rate_limit {
                    id "downstream.usage.day";
                    key [endpoint, "day" = day];
                    cost 2;
                    allow 5 per 1m;
                }
            }
        }
    }
}
