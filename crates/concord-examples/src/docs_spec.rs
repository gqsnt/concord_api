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
        timeout off;
        base "https://users.example.test/";


    GET user(id: u64) -> User using Json<User> {
        path ["v1", "users", endpoint.id];
    }
}}

use serde::Serialize;

pub mod fixed_base {
    use concord::api;

    api! {
        client ExampleApi {
            timeout off;
            base "https://api.example.test/v1/";
        }
    }
}

pub const DEFAULT_LABEL: Option<&'static str> = Some("documented");

pub mod values {
    use concord::api;

    api! {
        client ValuesApi {
            timeout off;
            base "https://values.example.test/";
            values {
                tenant: String;
                locale?: String;
                page_size: u32 = 25;
                #[default]
                tags: Vec<String>;
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
            timeout off;
            base "https://request-response.example.test/";


        POST update(
            id: String,
            body payload: UpdateInput using concord::Json<UpdateInput>,
        ) -> Item using Json<Item>;
    }}
}

pub mod identity {
    use concord::api;

    use super::Item;

    api! {
        client IdentityApi {
            timeout off;
            base "https://identity.example.test/";


        GET by_id(id: String) -> Item using Json<Item> {
            id "catalog.item.by-id";
        }
    }}
}

pub mod expressions {
    use concord::api;

    api! {
        client ExpressionApi {
            timeout off;
            base "https://expression.example.test/";


        scope regions(region: String) {
            GET item(id: String) -> () using NoContent {
                path [concat ["item-", endpoint.id, "-", regions.region]];
            }
        }
    }}
}

pub mod shaping {
    use concord::api;

    api! {
        client ShapingApi {
            timeout off;
            base "https://shaping.example.test/";
            values {
                locale?: String;
            }


        scope regions(region: String) {
            GET item(id: String, tags: Vec<String>) -> () using NoContent {
                subdomain replace [regions.region, "api"];
                path ["v1", "items", endpoint.id];
                query {
                    "locale" = client.locale;
                    "tag" = each endpoint.tags;
                }
                headers {
                    "X-Region" = regions.region;
                }
            }
        }
    }}
}

pub mod authentication {
    use concord::api;

    api! {
        client AuthenticationApi {
            timeout off;
            base "https://authentication.example.test/";
            credentials {
                secret token;
                secret user;
                secret password;

                credential api_token = api_key(secret.token);
                credential bearer_token = bearer(secret.token);
                credential login = basic(secret.user, secret.password);
            }


        GET bearer_auth() -> () using NoContent {
            auth { bearer bearer_token recover on unauthorized; }
        }
        GET basic_auth() -> () using NoContent {
            auth { basic login recover on unauthorized_or_forbidden; }
        }
        GET exact_auth() -> () using NoContent {
            auth { authorization = bearer_token; }
        }
        GET scheme_auth() -> () using NoContent {
            auth { authorization "Token" = bearer_token recover on unauthorized; }
        }
        GET header_auth() -> () using NoContent {
            auth { header "X-API-Key" = api_token; }
        }
        GET query_auth() -> () using NoContent {
            auth { query "token" = api_token; }
        }
    }}
}

pub mod timeouts {
    use concord::api;

    api! {
        client TimeoutApi {
            timeout off;
            base "https://timeout.example.test/";


        GET milliseconds() -> () using NoContent { timeout 250ms; }
        GET seconds() -> () using NoContent { timeout 5s; }
        GET minutes() -> () using NoContent { timeout 10m; }
        GET hours() -> () using NoContent { timeout 1h; }
        GET no_timeout() -> () using NoContent { timeout off; }
    }}
}

pub mod pagination {
    use concord::api;

    api! {
        client PaginationApi {
            timeout off;
            base "https://pagination.example.test/";


        GET page(
            offset: u64,
            page_size: std::num::NonZeroU32,
        ) -> Vec<u32> using Json<Vec<u32>> {
            query {
                "offset" = endpoint.offset;
                "page_size" = endpoint.page_size;
            }
            paginate with concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.page_size;
            }
        }
    }}
}

pub mod rate_limits {
    use concord::api;

    api! {
        client RateLimitApi {
            timeout off;
            base "https://rate-limit.example.test/";
            values {
                tenant: String;
            }
            rate_limits {
                account_reads(account) {
                    id "service.account.read";
                    key [host, account];
                    cost 1;
                    allow 100 per 1m;
                    allow 1_000 per 1h;
                }
            }


        scope accounts(account: String, region: String) {
            rate_limit account_reads(account = accounts.account) { cost 2; }

            GET item(item_id: String) -> () using NoContent {
                rate_limit {
                    id "service.item.read";
                    key [host, endpoint, client.tenant, accounts.region, endpoint.item_id];
                    cost 2;
                    allow 100 per 1m;
                }
            }
        }
    }}
}

pub mod quick_start_values {
    use concord::api;

    api! {
        client RegionalApi {
            timeout off;
            base "https://example.test/";
            values {
                region: String;
            }
            subdomain replace [client.region, "api"];


        GET health() -> serde_json::Value using Json<serde_json::Value> {
            path ["health"];
        }
    }}
}

pub mod quick_start_auth {
    use concord::api;

    api! {
        client PrivateApi {
            timeout off;
            base "https://private.example.test/";
            credentials {
                secret token;
                credential bearer_token = bearer(secret.token);
            }
            auth { bearer bearer_token; }


        GET account() -> serde_json::Value using Json<serde_json::Value>;
    }}
}

/// One client that exercises the complete surviving DSL families together.
pub mod kitchen_sink {
    use concord::api;

    use super::{Item, UpdateInput};

    api! {
        client KitchenSinkApi {
            base "https://kitchen-sink.example.test/root/";
            values {
                tenant: String;
                locale?: String;
            }
            credentials {
                secret api_key;
                secret bearer_token;
                secret username;
                secret password;
                credential application_key = api_key(secret.api_key);
                credential bearer = bearer(secret.bearer_token);
                credential login = basic(secret.username, secret.password);
                credential provider =
                    custom crate::authentication::extensions::ExampleProvider;
                credential acquired = endpoint token_endpoint;
            }
            rate_limits {
                tenant_reads(tenant) {
                    id "kitchen.tenant.read";
                    key [host, tenant];
                    allow 100 per 1m;
                }
            }
            subdomain replace ["api"];
            path ["v1"];
            query { "locale" = client.locale; }
            headers { "X-Client" = "kitchen-sink"; }
            auth { header "X-Api-Key" = application_key; }
            timeout 10s;
            rate_limit {
                id "kitchen.global";
                allow 1_000 per 1m;
            }


        GET token_endpoint() -> crate::authentication::extensions::ExampleToken using crate::authentication::extensions::ExampleTokenCodec;

        GET basic_account() -> Item using Json<Item> {
            auth replace { basic login recover on unauthorized; }
        }

        GET provider_account() -> Item using Json<Item> {
            auth replace { bearer provider recover on unauthorized; }
        }

        GET acquired_account() -> Item using Json<Item> {
            auth replace { bearer acquired; }
        }

        scope regions(region: String) {
            subdomain replace [regions.region, "api"];
            path ["regions", regions.region];
            headers { "X-Region" = regions.region; }
            rate_limit tenant_reads(tenant = client.tenant);

            POST search(
                category: String,
                offset: u64,
                page_size: std::num::NonZeroU32,
                body filter: UpdateInput using concord::Json<UpdateInput>,
            ) -> Vec<Item> using concord::Json<Vec<Item>> {
                id "kitchen.regions.search";
                path ["items"];
                query {
                    "category" = endpoint.category;
                    "offset" = endpoint.offset;
                    "page_size" = endpoint.page_size;
                }
                auth replace { bearer bearer recover on unauthorized; }
                timeout 5s;
                rate_limit {
                    id "kitchen.search";
                    key [endpoint, endpoint.category];
                    cost 2;
                    allow 100 per 1m;
                }
                paginate with concord::pagination::OffsetPagination {
                    offset -> endpoint.offset;
                    page_size -> endpoint.page_size;
                }
            }

            METHOD("PROPFIND") inspect(id: String) -> String using Text {
                path [concat ["item-", endpoint.id]];
                auth off;
                timeout off;
            }
        }
    }}
}
