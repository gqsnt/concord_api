//! One compile-checked vertical reference for the current Concord language.
//!
//! Required features: `json`, `rate-limit`, and `default-tls`. Construction is
//! inert; the example creates plans and pending terminals but performs no I/O.

use std::fmt;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use concord::auth::{CredentialMaterial, SecretCredential};
use concord::rate_limit::LocalRateLimiter;
use concord::{
    ApiResponse, AtomError, BoundedAtomWriter, Bytes, PageLimit, RequestAtom, SecretString, api,
};

pub type TenantId = String;
pub type Locale = String;
pub type Region = String;
pub type Language = String;
pub type Category = String;
pub type Tag = String;
pub type ItemId = String;
pub type CommandId = String;
pub type PageSize = NonZeroU32;

pub const DEFAULT_PAGE_SIZE: PageSize = match NonZeroU32::new(25) {
    Some(value) => value,
    None => unreachable!(),
};

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

#[derive(serde::Serialize)]
pub struct LoginInput {
    pub user: String,
}

#[derive(serde::Serialize)]
pub struct UpdateInput {
    pub title: String,
}

#[derive(serde::Deserialize)]
pub struct ItemSummary {
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct Item {
    pub id: String,
    pub title: String,
}

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

        subdomain ["api"];
        path [client.api_version];

        query {
            "tenant" = client.tenant;
            "locale" = client.locale;
        }

        headers {
            "X-Client-Version" = "2";
        }

        auth {
            header "X-Bootstrap-Key" = application_key;
        }

        rate_limits {
            policy tenant_reads(tenant) {
                id "catalog.tenant.read";
                key [origin, tenant];
                cost 1;
                allow 120 per 1m;
                allow 5_000 per 1h;
            }
        }

        /// Creates a session credential.
        POST authenticate(
            body input: LoginInput via Json,
        ) -> SessionToken via Json {
            id "catalog.auth.login";
            path ["auth", "session"];
            auth replace {
                header "X-Bootstrap-Key" = application_key;
            }
            effect non_idempotent;
        }

        scope regional(
            region: Region,
            language?: Language,
        ) {
            subdomain [regional.region];
            path ["regions", regional.region];

            headers {
                "Accept-Language" = regional.language;
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
                path ["items"];
                query {
                    "category" = endpoint.category;
                    "offset" = endpoint.offset;
                    "count" = endpoint.count;
                    "tag" = each endpoint.tags;
                }
                paginate via concord::pagination::OffsetPagination {
                    offset -> endpoint.offset;
                    page_size -> endpoint.count;
                }
            }

            /// Returns one item.
            GET item(id: ItemId) -> Item via Json {
                id "catalog.item.get";
                path ["items", endpoint.id];
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
                path ["items", endpoint.id];
                headers {
                    "Idempotency-Key" = endpoint.command_id;
                }
                effect idempotent;
                rate_limit {
                    id "catalog.item.write";
                    key [origin, endpoint, client.tenant];
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
        .set_tenant("tenant-a".to_owned())
        .set_locale(Some("fr-FR".to_owned()))
        .reset_api_version()
        .bootstrap_key(SecretString::try_new("example-bootstrap-key")?)
        .configure_http(|http| http.require_https().connect_timeout(Duration::from_secs(3)))
        .configure_runtime(|runtime| {
            runtime
                .response_body_limit(NonZeroUsize::new(8 * 1024 * 1024).expect("positive"))
                .pagination_max_pages(NonZeroU32::new(100).expect("positive"))
                .rate_limiter(Arc::new(LocalRateLimiter::new()))
        })
        .build()?;

    let login = LoginInput {
        user: "catalog-reader".to_owned(),
    };
    let _store_session = client.authenticate(login).store_as_session();

    let regional = client
        .regional("eu-west".to_owned())
        .set_language(Some("fr".to_owned()));
    let request = regional
        .item_summaries(vec!["stable".to_owned()])
        .set_category(Some("featured".to_owned()))
        .reset_count();
    // ANCHOR: catalog_planning
    let _plan = request.plan()?;
    let _pagination = request.paginate(PageLimit::CompleteWithin(
        NonZeroU32::new(20).expect("positive"),
    ));
    let _accepted: concord::endpoint::AcceptedBytesRequest<_> =
        regional.item("item-42".to_owned()).accepted_bytes();

    let endpoint = CatalogApi::unbound()
        .regional("eu-west".to_owned())
        .item("item-42".to_owned());
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _response: concord::endpoint::ResponseRequest<_> = request.response();
    let _metadata_shape: Option<ApiResponse<Bytes>> = None;
    // ANCHOR_END: catalog_planning
    Ok(())
}
// ANCHOR_END: catalog_usage
