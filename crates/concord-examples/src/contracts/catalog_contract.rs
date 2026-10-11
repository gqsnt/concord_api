//! The meaning-preserving v3 migration of the baseline v2.1 Catalog.
//!
//! Required features: `json`, `rate-limit`, and `default-tls`. Construction is
//! inert; the example creates plans and pending terminals but performs no I/O.
//!
//! This is the migration fixture compared against the captured v2.1 recording
//! (§24): it keeps the original explicit query/header blocks and their order,
//! the pinned endpoint IDs, `X-Client-Version: 2`, the redundant-looking
//! authentication replacement on `authenticate`, and the legacy rate labels
//! (`"tenant"` for the named policy formal, `"client.tenant"` for the inline
//! write key). The canonical extended v3 showcase is
//! [`super::catalog_v3`]; the two are intentionally different declarations.

use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use concord::auth::{CredentialMaterial, SecretCredential};
use concord::pagination::{OffsetInit, OffsetPagination};
use concord::rate_limit::LocalRateLimiter;
use concord::{
    ApiResponse, AtomError, BoundedAtomWriter, Bytes, Json, RequestAtom, SecretString, api,
};

/// Tenant identifier alias; aliases keep their target's role.
pub type TenantId = String;
/// Locale alias.
pub type Locale = String;
/// Region alias.
pub type Region = String;
/// Language alias.
pub type Language = String;
/// Category alias.
pub type Category = String;
/// Tag alias.
pub type Tag = String;
/// Item identifier alias.
pub type ItemId = String;
/// Durable command identifier alias.
pub type CommandId = String;
/// Page size alias.
pub type PageSize = NonZeroU32;

/// Default page size retained by each list request.
pub const DEFAULT_PAGE_SIZE: PageSize = match NonZeroU32::new(25) {
    Some(value) => value,
    None => unreachable!(),
};

/// The provider's wire API version path segment (independent of Concord's
/// own major version).
#[derive(Clone)]
pub struct ApiVersion(&'static str);

impl Default for ApiVersion {
    fn default() -> Self {
        Self("v2")
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl RequestAtom for ApiVersion {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(self.0)
    }
}

/// Login body.
#[derive(serde::Serialize)]
pub struct LoginInput {
    /// User name.
    pub user: String,
}

/// Update body.
#[derive(serde::Serialize)]
pub struct UpdateInput {
    /// New title.
    pub title: String,
}

/// One list entry.
#[derive(serde::Deserialize)]
pub struct ItemSummary {
    /// Item identifier.
    pub id: String,
}

/// One item.
#[derive(serde::Deserialize)]
pub struct Item {
    /// Item identifier.
    pub id: String,
    /// Title.
    pub title: String,
}

/// Session credential material returned by `authenticate`.
#[derive(Clone)]
pub struct SessionToken(SecretString);

impl CredentialMaterial for SessionToken {}

impl SecretCredential for SessionToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionToken(<redacted>)")
    }
}

impl<'de> serde::Deserialize<'de> for SessionToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        SecretString::try_new(value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

// ANCHOR: catalog_contract
api! {
    /// Typed client for the catalog service.
    client CatalogApi {
        base "https://catalog.example.test/root/";
        timeout 20s;

        values {
            /// Tenant whose catalog is visible.
            #[sensitive]
            tenant: TenantId;

            /// Optional localization preference.
            locale?: Locale;

            /// API version segment selected for this client snapshot.
            #[default]
            api_version: ApiVersion;
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
                cost 1;
                allow 120 per 1m;
                allow 5_000 per 1h;
            }
        }

        subdomain ["api"];
        path "{api_version}";

        query {
            "tenant" = tenant;
            "locale" = locale;
        }

        headers {
            "X-Client-Version" = "2";
        }

        auth {
            header "X-Bootstrap-Key" = application_key;
        }

        /// Creates a session credential.
        POST authenticate(
            body input: LoginInput via Json,
        ) -> SessionToken via Json {
            id "catalog.auth.login";
            path "auth/session";
            auth replace {
                header "X-Bootstrap-Key" = application_key;
            }
            effect non_idempotent;
        }

        scope regional(
            region: Region,
            language?: Language,
        ) {
            subdomain [region];
            path "regions/{region}";

            headers {
                "Accept-Language" = language;
            }

            auth replace {
                bearer session challenge unauthorized;
            }

            rate_limit tenant_reads(tenant = client.tenant);

            /// Returns one page of item summaries.
            GET item_summaries(
                category?: Category,
                offset: u64 = 0,
                count: PageSize = DEFAULT_PAGE_SIZE,
                tags: Vec<Tag>,
            ) -> Vec<ItemSummary> via Json {
                id "catalog.items.list";
                path "items";
                query {
                    "category" = category;
                    "offset" = offset;
                    "count" = count;
                    "tag" = each tags;
                }
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }

            /// Returns one item.
            GET item(id: ItemId) -> Item via Json {
                id "catalog.item.get";
                path "items/{id}";
                expose accepted_bytes;
            }

            /// Applies an idempotent update identified by `command_id`.
            PUT update_item(
                id: ItemId,
                #[sensitive]
                command_id: CommandId,
                body update: UpdateInput via Json,
            ) -> Item via Json {
                id "catalog.item.update";
                path "items/{id}";
                headers {
                    "Idempotency-Key" = command_id;
                }
                effect idempotent;
                rate_limit {
                    id "catalog.item.write";
                    key [origin, endpoint, "client.tenant" = client.tenant];
                    cost 5;
                    allow 30 per 1m;
                }
            }
        }
    }
}
// ANCHOR_END: catalog_contract

/// Compile-checks representative direct and unbound usage without polling any
/// execution terminal.
// ANCHOR: catalog_usage
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = CatalogApi::builder()
        .set_tenant("tenant-a")
        .set_locale("fr-FR")
        .reset_api_version()
        .bootstrap_key(SecretString::try_new("example-bootstrap-key")?)
        .configure_http(|http| http.require_https().connect_timeout(Duration::from_secs(3)))
        .configure_runtime(|runtime| {
            runtime
                .response_body_limit(8 * 1024 * 1024)
                .pagination_max_pages(100)
                .rate_limiter(Arc::new(LocalRateLimiter::new()))
        })
        .build()?;

    let login = LoginInput {
        user: "catalog-reader".to_owned(),
    };
    let _store_session: concord::auth::StoreRequest = client.authenticate(login).store_as_session();

    let regional = client.regional("eu-west").set_language("fr");
    let request = regional
        .item_summaries(vec!["stable".to_owned()])
        .set_category("featured")
        .reset_count();
    // ANCHOR: catalog_planning
    let _plan = request.plan()?;
    let _complete: concord::pagination::CollectAll<ItemSummary> =
        request.collect_all(NonZeroU32::new(20).expect("positive"));
    let _accepted: concord::endpoint::AcceptedBytesRequest =
        regional.item("item-42").accepted_bytes();

    let endpoint = CatalogApi::unbound().regional("eu-west").item("item-42");
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _response: concord::endpoint::ResponseRequest<Item> = request.response();
    let _metadata_shape: Option<ApiResponse<Bytes>> = None;
    // ANCHOR_END: catalog_planning
    Ok(())
}
// ANCHOR_END: catalog_usage
