//! Representative Concord v2.1 declarations and callers (migrate-v3 input).
//!
//! This file is ordinary v2.1 source. `tools/xtask/src/migration/tests.rs`
//! migrates it and asserts that the result is byte-for-byte
//! `tools/xtask/tests/migrated_v3/representative.rs`, which
//! `tools/xtask/tests/migrated_v3.rs` compiles through the real v3
//! `concord::api!` and executes (V3-O04): nested `Option` setters, constant
//! versus owner qualification, direct-placement ordering, inherited-key
//! replacement, `when_present`, scope-shared quota, legacy key labels, path
//! templates, pinned endpoint IDs and traversal terminals.

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;

use concord::endpoint::RequestPlan;
use concord::rate_limit::RateLimiter;
use concord::{ApiError, AtomEncoder, AtomError, BoundedAtomWriter, api};

/// Rust constants deliberately named like the `defaults` scope below.
pub mod defaults {
    use std::num::NonZeroU32;

    use concord::{AtomError, BoundedAtomWriter, RequestAtom};

    pub const TIER: u32 = 2;
    pub const PAGE: u32 = 25;
    pub const REGION: &str = "eu";
    pub const PAGE_SIZE: NonZeroU32 = match NonZeroU32::new(10) {
        Some(value) => value,
        None => unreachable!(),
    };

    /// A version segment whose payload comes from `Default`.
    #[derive(Default)]
    pub struct ApiVersion;

    impl RequestAtom for ApiVersion {
        fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
            output.push_str("v1")
        }
    }
}

/// Encodes a nested optional milestone: an inner `None` is the text `none`.
pub struct MilestoneAtom;

impl AtomEncoder<Option<u32>> for MilestoneAtom {
    fn write(value: &Option<u32>, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        match value {
            Some(number) => output.write(number),
            None => output.push_str("none"),
        }
    }
}

/// Zero-pads a report number to four digits.
pub struct Padded;

impl AtomEncoder<u32> for Padded {
    fn write(value: &u32, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(&format!("{value:04}"))
    }
}

use concord::prelude::Json;

api! {
    /// Nested optionals, ordering, inherited keys and scope-shared quota.
    client Goals {
        base "https://goals.example.test/";
        timeout 5s;
        response Json;

        values {
            query locale?: String;
            header("X-Trace") trace: String = "root";
            tenant: String;
        }

        query {
            "v" = "1";
            replace "v" = "2";
        }

        rate_limits {
            policy per_tenant(tenant) {
                id "goals.tenant";
                key [origin, "tenant" = tenant];
                cost 1;
                allow 10 per 1s;
                allow 100 per 1m;
            }
        }

        scope project(key: String) {
            path "projects/{key}";
            rate_limit per_tenant(tenant = client.tenant);

            GET milestones(query milestone?: Option<u32> via crate::representative::MilestoneAtom, query limit: u32 = 10) -> Vec<u32> {
                path "milestones";
            }

            GET mixed(a?: String, query b?: String, c?: String) -> Vec<u32> {
                path "mixed";
                query {
                    "a" = a;
                    append "z" = "1";
                    "c" = c;
                }
            }

            GET localized(locale_override?: String, trace?: String) -> Vec<u32> {
                path "localized";
                query { when_present "locale" = locale_override; }
                headers { replace "x-trace" = trace; }
            }

            GET shared(query lang?: String) -> Vec<u32> {
                path "shared";
                query { "language" = lang; }
                headers { "Accept-Language" = lang; }
            }

            DELETE remove(id: String) -> Vec<u32> {
                id "goals.project.remove";
                path "items/{id}";
                rate_limit { id "goals.write"; key [origin, endpoint, "client.tenant" = client.tenant]; cost 2; allow 5 per 1s; }
            }
        }
    }
}

api! {
    /// Rust constant paths beside a scope that shares their module's name.
    client Consts {
        base "https://consts.example.test/";
        timeout 5s;
        response Json;

        values {
            #[default]
            api_version: crate::representative::defaults::ApiVersion;
            region: String = crate::representative::defaults::REGION;
        }

        path "{api_version}";

        scope defaults(tier: u32 = crate::representative::defaults::TIER, query zone?: String) {
            path "tiers/{tier}";
            query {
                "region" = client.region;
            }

            GET items(query offset: u64 = 0, query count: u32 = crate::representative::defaults::PAGE) -> Vec<u32> {
                path "{client.api_version}/items/{defaults.tier}";
                query {
                    "tier" = defaults.tier;
                }
                paginate via concord::pagination::OffsetPagination {
                    init concord::pagination::OffsetInit { offset: offset, page_size: crate::representative::defaults::PAGE_SIZE };
                    cursor -> offset;
                }
            }
        }
    }
}

api! {
    /// Path templates, a fixed base path and IDs pinned across the naming change.
    client Api2URL {
        base "https://edge.example.test/root";
        timeout 5s;
        response Json;

        values {
            #[sensitive]
            header("X-Token") token: String;
        }

        subdomain ["api"];

        scope braces(tenant: String) {
            subdomain [tenant];
            path "braces";

            GET literal() -> Vec<u32> {
                id "api2url.braces.literal";
                path "a{{b}}/c}}}}d/v2-true";
            }

            GET adapted(report: u32) -> Vec<u32> {
                id "api2url.braces.adapted";
                path "reports/{report via crate::representative::Padded}";
            }

            GET keyed() -> Vec<u32> {
                id "api2url.braces.keyed";
            }

            GET replaced() -> Vec<u32> {
                id "api2url.braces.replaced";
                path replace "fixed";
            }

            GET cleared() -> Vec<u32> {
                id "api2url.braces.cleared";
                path off;
            }

            METHOD("PURGE") purge() -> Vec<u32> {
                id "api2url.braces.purge";
            }
        }
    }
}

/// Builds a `Goals` client with an explicit limiter selection.
pub fn goals(tenant: &str, limiter: Arc<dyn RateLimiter>) -> Result<Goals, concord::ClientBuildError> {
    Goals::builder()
        .set_tenant(tenant.to_owned())
        .replace_locale(Some("fr".to_owned()))
        .configure_runtime(|runtime| {
            runtime
                .stream_response_limit(NonZeroUsize::new(1024 * 1024).expect("positive").get())
                .rate_limiter(limiter)
        })
        .build()
}

/// v2.1 exact outer-slot replacement: `set_milestone(None)` clears the slot
/// and `set_milestone(Some(None))` selects a present inner `None`.
pub fn milestone_plans(client: &Goals) -> Vec<Result<RequestPlan, ApiError>> {
    let project = client.project("p1".to_owned());
    vec![
        project.milestones().replace_milestone(None).plan(),
        project.milestones().replace_milestone(Some(None)).plan(),
        project
            .milestones()
            .replace_milestone(Some(Some(7)))
            .set_limit(3)
            .plan(),
    ]
}

/// Mixed plain introductions and an append keep their v2.1 wire order.
pub fn ordering_plan(client: &Goals) -> Result<RequestPlan, ApiError> {
    client
        .project("p1".to_owned())
        .mixed()
        .replace_a(Some("A".to_owned()))
        .replace_b(Some("B".to_owned()))
        .replace_c(Some("C".to_owned()))
        .plan()
}

/// `when_present` keeps the inherited key when absent; a plain assignment
/// over an inherited header replaces it.
pub fn localized_plans(client: &Goals) -> Vec<Result<RequestPlan, ApiError>> {
    let project = client.project("p1".to_owned());
    vec![
        project.localized().plan(),
        project
            .localized()
            .replace_locale_override(Some("de".to_owned()))
            .replace_trace(Some("endpoint".to_owned()))
            .plan(),
    ]
}

/// One optional value used by three emissions.
pub fn shared_plan(client: &Goals) -> Result<RequestPlan, ApiError> {
    client
        .project("p1".to_owned())
        .shared()
        .replace_lang(Some("en".to_owned()))
        .plan()
}

/// Builds the constants client.
pub fn consts() -> Result<Consts, concord::ClientBuildError> {
    Consts::builder().build()
}

/// Plans one item page with scope and endpoint controls.
pub fn consts_plan(client: &Consts) -> Result<RequestPlan, ApiError> {
    client
        .defaults()
        .replace_zone(Some("z1".to_owned()))
        .items()
        .set_offset(5)
        .plan()
}

/// A bounded prefix keeps the v2.1 `Vec` result.
pub async fn first_pages(client: &Consts) -> Result<Vec<u32>, ApiError> {
    let items = client.defaults().items().collect_prefix(2).await?.into_items();
    Ok(items)
}

/// Complete collection remains an error, never a partial `Vec`, at the bound.
pub async fn all_pages(client: &Consts) -> Result<Vec<u32>, ApiError> {
    client
        .defaults()
        .items()
        .collect_all(NonZeroU32::new(3).expect("positive"))
        .await
}

/// Builds the edge client.
pub fn edge() -> Result<Api2URL, concord::ClientBuildError> {
    Api2URL::builder().set_token("edge-token".to_owned()).build()
}

/// Plans every edge endpoint in declaration order.
pub fn edge_plans(client: &Api2URL) -> Vec<Result<RequestPlan, ApiError>> {
    let braces = client.braces("acme".to_owned());
    vec![
        braces.literal().plan(),
        braces.adapted(7).plan(),
        braces.keyed().plan(),
        braces.replaced().plan(),
        braces.cleared().plan(),
        braces.purge().plan(),
    ]
}

/// The captured v2.1 Catalog contract declaration (verbatim), compiled after
/// migration and compared with the v2.1 limiter recording (V3-R02 labels).
pub mod catalog {
    use std::fmt;
    use std::num::NonZeroU32;
    use std::sync::Arc;

    use concord::auth::{CredentialMaterial, SecretCredential};
    use concord::rate_limit::RateLimiter;
    use concord::{AtomError, BoundedAtomWriter, RequestAtom, SecretString, api};

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

    impl ApiVersion {
        /// An explicitly selected version segment.
        pub const fn new(segment: &'static str) -> Self {
            Self(segment)
        }
    }

    thread_local! {
        /// `ApiVersion::default()` evaluations on this thread; the migrated
        /// test classifies the v2.1 -> v3 root-default timing change with it.
        pub static API_VERSION_DEFAULTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    impl Default for ApiVersion {
        fn default() -> Self {
            API_VERSION_DEFAULTS.with(|count| count.set(count.get() + 1));
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

    use concord::prelude::Json;

    api! {
        /// Typed client for the catalog service.
        client CatalogApi {
            base "https://catalog.example.test/root/";
            timeout 20s;

            values {
                /// Tenant whose catalog is visible.
                #[sensitive]
                query tenant: TenantId;

                /// Optional localization preference.
                query locale?: Locale;

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
            path "{api_version}";

            headers {
                "X-Client-Version" = "2";
            }

            auth {
                header "X-Bootstrap-Key" = application_key;
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
                header("Accept-Language") language?: Language,
            ) {
                subdomain [region];
                path "regions/{region}";

                auth replace {
                    bearer session challenge unauthorized;
                }

                rate_limit tenant_reads(tenant = client.tenant);

                /// Returns one page of item summaries.
                GET item_summaries(
                    query category?: Category,
                    query offset: u64 = 0,
                    query count: PageSize = DEFAULT_PAGE_SIZE,
                    query("tag") each tags: Vec<Tag>,
                ) -> Vec<ItemSummary> via Json {
                    id "catalog.items.list";
                    path "items";
                    paginate via concord::pagination::OffsetPagination {
                        init concord::pagination::OffsetInit { offset: offset, page_size: count };
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
                    header("Idempotency-Key") command_id: CommandId,
                    body update: UpdateInput via Json,
                ) -> Item via Json {
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
            }
        }
    }

    /// Builds the Catalog client for one tenant and an explicit limiter.
    pub fn client(
        tenant: &str,
        limiter: Arc<dyn RateLimiter>,
    ) -> Result<CatalogApi, concord::ClientBuildError> {
        CatalogApi::builder()
            .set_tenant(tenant.to_owned())
            .replace_locale(None)
            .bootstrap_key(SecretString::try_new("catalog-bootstrap-sentinel").expect("bounded"))
            .configure_runtime(|runtime| runtime.rate_limiter(limiter))
            .build()
    }
}
