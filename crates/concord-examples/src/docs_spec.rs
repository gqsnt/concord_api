//! Compile-checked source for every fenced Concord DSL example.
//!
//! Rust `concord-xtask docs-check` owns the documentation contract used by
//! local and CI verification. It requires each unanchored `concord` fence to
//! match a normalized substring of this compiled module.
//!
//! Every snippet is canonical v3: relative `path` templates, bare local names
//! with explicit ancestor qualification, direct query/header placement,
//! explicit rate labels, `init`/`cursor` pagination and imported
//! representation types. The root timeout is omitted where the visible
//! 30-second default is intended.

use concord::{AtomEncoder, AtomError, BoundedAtomWriter, Json, api};
use serde::{Deserialize, Serialize};

/// A user returned by the first documented declaration.
#[derive(Deserialize)]
pub struct User {
    /// User identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
}

api! {
    client UsersApi {
        base "https://users.example.test/";

        GET user(id: u64) -> User via Json {
            path "v1/users/{id}";
        }
    }
}

/// A fixed base with a generated path prefix and no endpoints yet.
pub mod fixed_base {
    use concord::api;

    api! {
        client ExampleApi {
            base "https://api.example.test/v1/";
        }
    }
}

/// Retained default of the documented inner-optional `label` value.
pub const DEFAULT_LABEL: Option<&'static str> = Some("documented");

/// Explicit atom adapter for the documented inner-optional label: inner
/// `None` is domain data rendered as `none`, not a generic omission rule.
pub struct LabelOrNone;

impl AtomEncoder<Option<&'static str>> for LabelOrNone {
    fn write(
        value: &Option<&'static str>,
        out: &mut BoundedAtomWriter<'_>,
    ) -> Result<(), AtomError> {
        out.push_str(value.unwrap_or("none"))
    }
}

/// Every value-state shape of §4.1 at the root.
pub mod values {
    use concord::api;

    use super::LabelOrNone;

    api! {
        client ValuesApi {
            base "https://values.example.test/";
            values {
                query tenant: String;
                query locale?: String;
                query page_size: u32 = 25;
                #[default]
                query("tag") each tags: Vec<String>;
                header("X-Label") label: Option<&'static str> via LabelOrNone = crate::DEFAULT_LABEL;
            }
        }
    }
}

/// Update body used by the request/response snippets.
#[derive(Clone, Serialize)]
pub struct UpdateInput {
    /// New name.
    pub name: String,
}

/// Item output used by the request/response snippets.
#[derive(Deserialize)]
pub struct Item {
    /// Item identifier.
    pub id: String,
}

/// An exact body input and a typed result.
pub mod request_response {
    use concord::{Json, api};

    use super::{Item, UpdateInput};

    api! {
        client RequestResponseApi {
            base "https://request-response.example.test/";

            POST update(id: String, body payload: UpdateInput via Json) -> Item via Json {
                path "items/{id}";
            }
        }
    }
}

/// An explicitly pinned operational endpoint ID.
pub mod identity {
    use concord::{Json, api};

    use super::Item;

    api! {
        client IdentityApi {
            base "https://identity.example.test/";

            GET by_id(id: String) -> Item via Json {
                id "catalog.item.by-id";
                path "items/{id}";
            }
        }
    }
}

/// A path segment mixing literals, a local hole and a qualified ancestor hole.
pub mod expressions {
    use concord::{Empty, api};

    api! {
        client ExpressionApi {
            base "https://expression.example.test/";

            scope regions(region: String) {
                GET item(id: String) -> () via Empty {
                    path "item-{id}-{regions.region}";
                }
            }
        }
    }
}

/// Explicit ordered query/header programs referencing local and ancestor values.
pub mod shaping {
    use concord::{Empty, api};

    api! {
        client ShapingApi {
            base "https://shaping.example.test/";
            values {
                locale?: String;
            }

            scope regions(region: String) {
                subdomain replace [region, "api"];

                GET item(id: String, tags: Vec<String>) -> () via Empty {
                    path "v1/items/{id}";
                    query {
                        "locale" = client.locale;
                        "tag" = each tags;
                    }
                    headers {
                        "X-Region" = regions.region;
                    }
                }
            }
        }
    }
}

/// Every static credential placement form.
pub mod authentication {
    use concord::{Empty, api};

    api! {
        client AuthenticationApi {
            base "https://authentication.example.test/";
            credentials {
                secret token;
                secret user;
                secret password;

                credential api_token = api_key(secret.token);
                credential bearer_token = bearer(secret.token);
                credential login = basic(secret.user, secret.password);
            }

            GET bearer_auth() -> () via Empty {
                auth { bearer bearer_token challenge unauthorized; }
            }
            GET basic_auth() -> () via Empty {
                auth { basic login challenge unauthorized_or_forbidden; }
            }
            GET exact_auth() -> () via Empty {
                auth { authorization = bearer_token; }
            }
            GET scheme_auth() -> () via Empty {
                auth { authorization "Token" = bearer_token challenge unauthorized; }
            }
            GET header_auth() -> () via Empty {
                auth { header "X-API-Key" = api_token; }
            }
            GET query_auth() -> () via Empty {
                auth { query "token" = api_token; }
            }
        }
    }
}

/// Duration units; the nearest finite selection wins.
pub mod timeouts {
    use concord::{Empty, api};

    api! {
        client TimeoutApi {
            base "https://timeout.example.test/";
            timeout 30s;

            GET milliseconds() -> () via Empty { timeout 250ms; }
            GET seconds() -> () via Empty { timeout 5s; }
            GET minutes() -> () via Empty { timeout 10m; }
            GET hours() -> () via Empty { timeout 1h; }
            GET no_timeout() -> () via Empty { timeout 30s; }
        }
    }
}

/// Offset pagination through a public `init` type and a whole-slot cursor.
pub mod pagination {
    use concord::{Json, api};

    api! {
        client PaginationApi {
            base "https://pagination.example.test/";

            GET page(
                query offset: u64,
                query page_size: std::num::NonZeroU32,
            ) -> Vec<u32> via Json {
                paginate via concord::pagination::OffsetPagination {
                    init concord::pagination::OffsetInit { offset: offset, page_size: page_size };
                    cursor -> offset;
                }
            }
        }
    }
}

/// Named and inline rate policies with explicit ordered key labels.
pub mod rate_limits {
    use concord::{Empty, api};

    api! {
        client RateLimitApi {
            base "https://rate-limit.example.test/";
            values {
                tenant: String;
            }
            rate_limits {
                policy account_reads(account) {
                    id "service.account.read";
                    key [origin, "account" = account];
                    cost 1;
                    allow 100 per 1m;
                    allow 1_000 per 1h;
                }
            }

            scope accounts(account: String, region: String) {
                rate_limit account_reads(account = account) { cost 2; }

                GET item(item_id: String) -> () via Empty {
                    id "rate_limit_api.accounts.item";
                    rate_limit {
                        id "service.item.read";
                        key [
                            origin,
                            endpoint,
                            "client.tenant" = client.tenant,
                            "accounts.region" = accounts.region,
                            "endpoint.item_id" = item_id,
                        ];
                        cost 2;
                        allow 100 per 1m;
                    }
                }
            }
        }
    }
}

/// A root value used as a generated DNS label.
pub mod quick_start_values {
    use concord::{Json, api};

    api! {
        client RegionalApi {
            base "https://example.test/";
            values {
                region: String;
            }
            subdomain [region, "api"];

            GET health() -> serde_json::Value via Json {
                path "health";
            }
        }
    }
}

/// Root bearer authentication.
pub mod quick_start_auth {
    use concord::{Json, api};

    api! {
        client PrivateApi {
            base "https://private.example.test/";
            credentials {
                secret token;
                credential bearer_token = bearer(secret.token);
            }
            auth { bearer bearer_token; }

            GET account() -> serde_json::Value via Json;
        }
    }
}

/// One client that exercises the complete surviving DSL families together.
pub mod kitchen_sink {
    use std::num::NonZeroU32;

    use concord::pagination::{OffsetInit, OffsetPagination};
    use concord::{Json, Text, api};

    use super::{Item, UpdateInput};
    use crate::authentication::extensions::{ExampleProvider, ExampleToken, ExampleTokenCodec};

    api! {
        client KitchenSinkApi {
            base "https://kitchen-sink.example.test/root/";
            timeout 10s;
            values {
                tenant: String;
                query locale?: String;
            }
            credentials {
                secret api_key;
                secret bearer_token;
                secret username;
                secret password;
                credential application_key = api_key(secret.api_key);
                credential bearer = bearer(secret.bearer_token);
                credential login = basic(secret.username, secret.password);
                credential provider = custom ExampleProvider;
                credential acquired = endpoint token_endpoint;
            }
            rate_limits {
                policy tenant_reads(tenant) {
                    id "kitchen.tenant.read";
                    key [origin, "tenant" = tenant];
                    cost 1;
                    allow 100 per 1m;
                }
            }
            subdomain ["api"];
            path "v1";
            headers { "X-Client" = "kitchen-sink"; }
            auth { header "X-Api-Key" = application_key; }
            rate_limit {
                id "kitchen.global";
                key [origin];
                cost 1;
                allow 1_000 per 1m;
            }

            GET token_endpoint() -> ExampleToken via ExampleTokenCodec;

            GET basic_account() -> Item via Json {
                auth replace { basic login challenge unauthorized; }
            }

            GET provider_account() -> Item via Json {
                auth replace { bearer provider challenge unauthorized; }
            }

            GET acquired_account() -> Item via Json {
                auth replace { bearer acquired; }
            }

            scope regions(region: String) {
                subdomain replace [region, "api"];
                path "regions/{region}";
                headers { "X-Region" = region; }
                rate_limit tenant_reads(tenant = client.tenant);

                POST search(
                    query category: String,
                    query offset: u64,
                    query page_size: NonZeroU32,
                    body filter: UpdateInput via Json,
                ) -> Vec<Item> via Json {
                    id "kitchen.regions.search";
                    effect safe;
                    path "items";
                    auth replace { bearer bearer challenge unauthorized; }
                    timeout 5s;
                    rate_limit {
                        id "kitchen.search";
                        key [endpoint, "endpoint.category" = category];
                        cost 2;
                        allow 100 per 1m;
                    }
                    paginate via OffsetPagination {
                        init OffsetInit { offset: offset, page_size: page_size };
                        cursor -> offset;
                    }
                }

                METHOD("PROPFIND") inspect(id: String) -> String via Text {
                    path "item-{id}";
                    auth off;
                    timeout 30s;
                }
            }
        }
    }
}

/// The first-contact declaration of the v3 specification (§2.1).
pub mod first_contact {
    use concord::{Json, api};

    use super::Item;

    api! {
        client Catalog {
            base "https://catalog.example.test/v1/";
            response Json;
            values {
                query tenant: String;
            }

            scope regional(region: String) {
                path "regions/{region}";

                /// Returns one page of items. Ordinary await does not paginate.
                GET items(
                    query category?: String,
                    query count: u32 = 25,
                    #[default] query("tag") each tags: Vec<String>,
                ) -> Vec<Item> {
                    path "items";
                }

                /// Returns None only for the declared missing-resource status.
                GET find_item(id: String) -> Option<Item> {
                    path "items/{id}";
                    absent on 404;
                }
            }
        }
    }
}

/// One caller-controlled optional with several selected uses (§2.2).
pub mod several_uses {
    use concord::{Json, api};

    use super::Item;

    api! {
        client SeveralUsesApi {
            base "https://several-uses.example.test/";
            response Json;

            GET inspect(query locale?: String) -> Item {
                path "inspect";
                headers {
                    "Accept-Language" = locale;
                }
                query {
                    "language" = locale;
                }
            }
        }
    }
}

/// Every value-state shape of §4.1.
pub mod value_states {
    use concord::api;

    api! {
        client ValueStatesApi {
            base "https://value-states.example.test/";
            values {
                query tenant: String;
                query locale?: String;
                query page_size: u32 = 25;
                query region?: String = "eu";
                #[default] query("tag") each tags: Vec<String>;
                #[default] query("flag") each flags?: Vec<String>;
            }
        }
    }
}

// ANCHOR: atom_encoder_extension
/// A calendar day rendered by the explicit [`IsoDate`] path adapter.
pub struct Day {
    /// Four-digit year.
    pub year: u16,
    /// Month, 1-12.
    pub month: u8,
    /// Day of month, 1-31.
    pub day: u8,
}

/// Explicit atom adapter rendering a [`Day`] as `YYYY-MM-DD`.
pub struct IsoDate;

impl AtomEncoder<Day> for IsoDate {
    fn write(value: &Day, out: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        out.push_str(&format!(
            "{:04}-{:02}-{:02}",
            value.year, value.month, value.day
        ))
    }
}
// ANCHOR_END: atom_encoder_extension

/// Relative path templates and DNS label lists (§5).
pub mod routes {
    use concord::{Empty, api};

    use super::{Day, IsoDate};

    api! {
        client RouteApi {
            base "https://route.example.test/root";
            values {
                api_version: String = "v2";
            }
            subdomain ["api"];
            path "{api_version}";

            scope regional(region: String) {
                subdomain [region];
                path "regions/{region}";

                GET item(id: String) -> () via Empty {
                    path "items/{id}";
                }
                GET details(id: String) -> () via Empty {
                    path "resource-{id}/details";
                }
                GET report(date: Day) -> () via Empty {
                    path "reports/{date via IsoDate}";
                }
                GET legacy() -> () via Empty {
                    path replace "legacy/{client.api_version}";
                }
                GET origin_root() -> () via Empty {
                    path off;
                }
                GET braces() -> () via Empty {
                    path "literal-{{braces}}";
                }
            }

            scope global() {
                subdomain replace ["global"];
                GET status() -> () via Empty;
            }

            scope apex() {
                subdomain off;
                GET ping() -> () via Empty;
            }
        }
    }
}

/// The ordered query/header patch program (§6): parent entries
/// `a=1, locale=fr, z=9` and each keyed operation over them.
pub mod patch_program {
    use concord::{Empty, api};

    api! {
        client PatchApi {
            base "https://patch.example.test/";
            values {
                locale: String = "fr";
            }
            query {
                "a" = "1";
                "locale" = locale;
                "z" = "9";
            }

            GET conditional(override_locale?: String) -> () via Empty {
                query { when_present "locale" = override_locale; }
            }
            GET exact(override_locale?: String) -> () via Empty {
                query { replace "locale" = override_locale; }
            }
            GET appended(extra_locale?: String) -> () via Empty {
                query { append "locale" = extra_locale; }
            }
            GET removed() -> () via Empty {
                query { remove "locale"; }
            }
            GET fresh(query page?: u32) -> () via Empty {
                query replace { "only" = "this"; }
            }
            GET bare() -> () via Empty {
                query off;
            }
        }
    }
}

/// Accepted statuses, effects, head-only absence and accepted bytes (§10).
pub mod outcomes {
    use concord::{Discard, Json, Text, api};

    use super::{Item, UpdateInput};

    api! {
        client OutcomeApi {
            base "https://outcome.example.test/";
            response Json;

            GET find(id: String) -> Option<Item> {
                path "items/{id}";
                absent on 404, 410;
            }
            GET strict(id: String) -> Item {
                path "strict/{id}";
                expose accepted_bytes;
            }
            POST create(body input: UpdateInput via Json) -> Item {
                path "items";
                status 201;
            }
            PUT upsert(id: String, body input: UpdateInput via Json) -> Item {
                path "items/{id}";
                status [200..=201, 204];
                effect idempotent;
            }
            DELETE delete(id: String) -> () via Discard {
                path "items/{id}";
                status success;
            }
            METHOD("PROPFIND") properties(id: String) -> String via Text {
                path "items/{id}";
                effect safe;
            }
        }
    }
}

/// A reusable numeric window set with explicit, separate identities (§12.1).
pub mod window_sets {
    use concord::{Empty, api};

    api! {
        client WindowSetApi {
            base "https://window-set.example.test/";
            values {
                header("X-Tenant") tenant: String;
            }
            rate_limits {
                windows standard { allow 20000 per 10s; allow 1200000 per 10m; }
                policy tenant_reads(tenant) {
                    id "catalog.tenant.read";
                    key [origin, "tenant" = tenant];
                    allow 120 per 1m;
                    allow 5000 per 1h;
                }
                policy shared_reads() {
                    id "catalog.shared.read";
                    key [origin];
                    using standard;
                }
            }
            rate_limit "concord.riot.21" key [origin] using standard;

            scope accounts() {
                rate_limit tenant_reads(tenant = client.tenant);
                rate_limit shared_reads();

                GET list() -> () via Empty;
            }
        }
    }
}

/// The URL and query tables of `docs/requests_and_values.md` are asserted
/// against real inert plans of the snippets above.
#[cfg(test)]
mod tests {
    use super::patch_program::PatchApi;
    use super::routes::RouteApi;
    use super::{Day, outcomes::OutcomeApi, several_uses::SeveralUsesApi};

    fn url(plan: Result<concord::endpoint::RequestPlan, concord::ApiError>) -> String {
        plan.expect("inert plan").observed_url().to_owned()
    }

    #[test]
    fn route_table_matches_planned_urls() {
        let client = RouteApi::new().expect("route client");
        let regional = client.regional("eu");
        assert_eq!(
            url(regional.item("7").plan()),
            "https://eu.api.route.example.test/root/v2/regions/eu/items/7"
        );
        assert_eq!(
            url(regional.details("7").plan()),
            "https://eu.api.route.example.test/root/v2/regions/eu/resource-7/details"
        );
        assert_eq!(
            url(regional
                .report(Day {
                    year: 2026,
                    month: 9,
                    day: 26,
                })
                .plan()),
            "https://eu.api.route.example.test/root/v2/regions/eu/reports/2026-09-26"
        );
        assert_eq!(
            url(regional.legacy().plan()),
            "https://eu.api.route.example.test/root/legacy/v2"
        );
        assert_eq!(
            url(regional.origin_root().plan()),
            "https://eu.api.route.example.test/root/"
        );
        assert_eq!(
            url(regional.braces().plan()),
            "https://eu.api.route.example.test/root/v2/regions/eu/literal-%7Bbraces%7D"
        );
        assert_eq!(
            url(client.global().status().plan()),
            "https://global.route.example.test/root/v2"
        );
        assert_eq!(
            url(client.apex().ping().plan()),
            "https://route.example.test/root/v2"
        );
    }

    #[test]
    fn patch_program_table_matches_planned_queries() {
        let client = PatchApi::new().expect("patch client");
        let base = "https://patch.example.test/";
        assert_eq!(
            url(client.conditional().plan()),
            format!("{base}?a=1&locale=fr&z=9")
        );
        assert_eq!(
            url(client.conditional().set_override_locale("de").plan()),
            format!("{base}?a=1&z=9&locale=de")
        );
        assert_eq!(url(client.exact().plan()), format!("{base}?a=1&z=9"));
        assert_eq!(
            url(client.exact().set_override_locale("de").plan()),
            format!("{base}?a=1&z=9&locale=de")
        );
        assert_eq!(
            url(client.appended().set_extra_locale("de").plan()),
            format!("{base}?a=1&locale=fr&z=9&locale=de")
        );
        assert_eq!(url(client.removed().plan()), format!("{base}?a=1&z=9"));
        assert_eq!(
            url(client.fresh().set_page(2).plan()),
            format!("{base}?page=2&only=this")
        );
        assert_eq!(url(client.bare().plan()), base);
    }

    #[test]
    fn one_optional_feeds_every_selected_use() {
        let client = SeveralUsesApi::new().expect("several-uses client");
        let present = client.inspect().set_locale("de").plan().expect("plan");
        assert_eq!(
            present.observed_url(),
            "https://several-uses.example.test/inspect?locale=de&language=de"
        );
        assert!(
            present
                .public_headers()
                .iter()
                .any(|(name, _)| name.as_str() == "accept-language")
        );
        let absent = client.inspect().plan().expect("plan");
        assert_eq!(
            absent.observed_url(),
            "https://several-uses.example.test/inspect"
        );
        assert!(
            !absent
                .public_headers()
                .iter()
                .any(|(name, _)| name.as_str() == "accept-language")
        );
    }

    #[test]
    fn outcome_declarations_select_the_documented_capabilities() {
        let client = OutcomeApi::new().expect("outcome client");
        let _bytes: concord::endpoint::AcceptedBytesRequest = client.strict("7").accepted_bytes();
        let plan = client.find("7").plan().expect("plan");
        assert!(plan.absent_status().is_some());
    }
}
