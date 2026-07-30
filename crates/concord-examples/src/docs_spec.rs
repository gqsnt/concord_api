//! Compile-checked source for every fenced Concord DSL example.
//!
//! `scripts/check_docs_examples.ps1` requires each `concord` fence under
//! `docs/` to match a normalized substring of this file.

use concord::{Json, api};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

api! {
    client UsersApi {
        base "https://users.example.test/";
    }

    GET user(id: u64) -> Json<User> {
        path ["v1", "users", ep.id];
    }
}

use serde::Serialize;

pub mod fixed_base {
    use concord::api;

    api! {
        client ExampleApi {
            base "https://api.example.test/v1/";
        }
    }
}

pub const DEFAULT_LABEL: Option<&'static str> = Some("documented");

pub mod values {
    use concord::api;

    api! {
        client ValuesApi {
            base "https://values.example.test/";
            vars {
                tenant: String;
                locale: Option<String>;
                page_size: u32 = 25;
                tags: Vec<String> = default;
                label: Option<&'static str> = crate::DEFAULT_LABEL;
            }
        }
    }
}

#[derive(Clone, Serialize)]
pub struct UpdateInput {
    pub name: String,
}

#[derive(Deserialize)]
pub struct Item {
    pub id: String,
}

pub mod request_response {
    use concord::api;

    use super::{Item, UpdateInput};

    api! {
        client RequestResponseApi {
            base "https://request-response.example.test/";
        }

        POST update(
            id: String,
            #[body] payload: Json<UpdateInput>,
        ) -> Json<Item>;
    }
}

pub mod identity {
    use concord::api;

    use super::Item;

    api! {
        client IdentityApi {
            base "https://identity.example.test/";
        }

        GET by_id(id: String) -> Json<Item> {
            id "catalog.item.by-id";
        }
    }
}

pub mod expressions {
    use concord::api;

    api! {
        client ExpressionApi {
            base "https://expression.example.test/";
        }

        scope regions(region: String) {
            GET item(id: String) -> NoContent {
                path [fmt["item-", ep.id, "-", scope.region]];
            }
        }
    }
}

pub mod shaping {
    use concord::api;

    api! {
        client ShapingApi {
            base "https://shaping.example.test/";
            vars {
                locale: Option<String>;
            }
        }

        scope regions(region: String) {
            GET item(id: String, tags: Vec<String>) -> NoContent {
                host [scope.region, "api"];
                path ["v1", "items", ep.id];
                query {
                    "locale" = vars.locale;
                    "tag" = ep.tags;
                }
                headers {
                    "X-Region" = scope.region;
                }
            }
        }
    }
}

pub mod authentication {
    use concord::api;

    api! {
        client AuthenticationApi {
            base "https://authentication.example.test/";
            credentials {
                secret token: String;
                secret user: String;
                secret password: String;

                credential api_token = api_key(secret.token);
                credential bearer_token = bearer(secret.token);
                credential login = basic(secret.user, secret.password);
            }
        }

        GET bearer_auth() -> NoContent {
            auth bearer bearer_token challenge unauthorized;
        }
        GET basic_auth() -> NoContent {
            auth basic login challenge unauthorized_or_forbidden;
        }
        GET exact_auth() -> NoContent {
            auth authorization = bearer_token challenge never;
        }
        GET scheme_auth() -> NoContent {
            auth authorization "Token" = bearer_token challenge unauthorized;
        }
        GET header_auth() -> NoContent {
            auth header "X-API-Key" = api_token challenge never;
        }
        GET query_auth() -> NoContent {
            auth query "token" = api_token;
        }
    }
}

pub mod timeouts {
    use concord::api;

    api! {
        client TimeoutApi {
            base "https://timeout.example.test/";
        }

        GET milliseconds() -> NoContent { timeout 250ms; }
        GET seconds() -> NoContent { timeout 5s; }
        GET minutes() -> NoContent { timeout 10m; }
        GET hours() -> NoContent { timeout 1h; }
        GET no_timeout() -> NoContent { timeout off; }
    }
}

pub mod pagination {
    use concord::api;

    api! {
        client PaginationApi {
            base "https://pagination.example.test/";
        }

        GET page(
            offset: u64,
            page_size: std::num::NonZeroU32,
        ) -> Json<Vec<u32>> {
            query {
                "offset" = ep.offset;
                "page_size" = ep.page_size;
            }
            paginate concord::pagination::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.page_size,
            }
        }
    }
}

pub mod rate_limits {
    use concord::api;

    api! {
        client RateLimitApi {
            base "https://rate-limit.example.test/";
            vars {
                tenant: String;
            }
            rate_limits {
                rule account_reads {
                    id "service.account.read";
                    by [host, account];
                    cost 1;
                    100 / 1m;
                    1_000 / 1h;
                }
            }
        }

        scope accounts(account: String, region: String) {
            rate_limit use account_reads {
                account = scope.account;
                cost 2;
            }

            GET item(item_id: String) -> NoContent {
                rate_limit {
                    id "service.item.read";
                    by [host, endpoint, vars.tenant, scope.region, ep.item_id];
                    cost 2;
                    100 / 1m;
                }
            }
        }
    }
}

pub mod quick_start_values {
    use concord::api;

    api! {
        client RegionalApi {
            base "https://example.test/";
            vars {
                region: String;
            }
            host [vars.region, "api"];
        }

        GET health() -> Json<serde_json::Value> {
            path ["health"];
        }
    }
}

pub mod quick_start_auth {
    use concord::api;

    api! {
        client PrivateApi {
            base "https://private.example.test/";
            credentials {
                secret token: String;
                credential bearer_token = bearer(secret.token);
            }
            auth bearer bearer_token challenge never;
        }

        GET account() -> Json<serde_json::Value>;
    }
}

/// One client that exercises the complete surviving DSL families together.
pub mod kitchen_sink {
    use concord::api;

    use super::{Item, UpdateInput};

    api! {
        client KitchenSinkApi {
            base "https://kitchen-sink.example.test/root/";
            vars {
                tenant: String;
                locale: Option<String>;
            }
            credentials {
                secret api_key: String;
                secret bearer_token: String;
                secret username: String;
                secret password: String;
                credential application_key = api_key(secret.api_key);
                credential bearer = bearer(secret.bearer_token);
                credential login = basic(secret.username, secret.password);
                credential provider =
                    custom crate::authentication::extensions::ExampleProvider;
                credential acquired = endpoint token_endpoint;
            }
            rate_limits {
                rule tenant_reads {
                    id "kitchen.tenant.read";
                    by [host, tenant];
                    100 / 1m;
                }
            }
            host ["api"];
            path ["v1"];
            query { "locale" = vars.locale; }
            headers { "X-Client" = "kitchen-sink"; }
            auth header "X-Api-Key" = application_key challenge never;
            timeout 10s;
            rate_limit {
                id "kitchen.global";
                1_000 / 1m;
            }
        }

        GET token_endpoint() -> crate::authentication::extensions::ExampleTokenCodec;

        GET basic_account() -> Json<Item> {
            auth replace basic login challenge unauthorized;
        }

        GET provider_account() -> Json<Item> {
            auth replace bearer provider challenge unauthorized;
        }

        GET acquired_account() -> Json<Item> {
            auth replace bearer acquired challenge never;
        }

        scope regions(region: String) {
            host [scope.region, "api"];
            path ["regions", scope.region];
            headers { "X-Region" = scope.region; }
            rate_limit use tenant_reads {
                tenant = vars.tenant;
            }

            POST search(
                category: String,
                offset: u64,
                page_size: std::num::NonZeroU32,
                #[body] filter: Json<UpdateInput>,
            ) -> Json<Vec<Item>> {
                id "kitchen.regions.search";
                path ["items"];
                query {
                    "category" = ep.category;
                    "offset" = ep.offset;
                    "page_size" = ep.page_size;
                }
                auth replace bearer bearer challenge unauthorized;
                timeout 5s;
                rate_limit {
                    id "kitchen.search";
                    by [endpoint, ep.category];
                    cost 2;
                    100 / 1m;
                }
                paginate concord::pagination::OffsetPagination {
                    offset: bind ep.offset,
                    page_size: bind ep.page_size,
                }
            }

            METHOD("PROPFIND") inspect(id: String) -> Text {
                path [fmt["item-", ep.id]];
                auth off;
                timeout off;
            }
        }
    }
}
