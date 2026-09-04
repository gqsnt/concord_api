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
        timeout 30s;
        base "https://users.example.test/";


    GET user(id: u64) -> User via Json {
        path ["v1", "users", endpoint.id];
    }
}}

use serde::Serialize;

pub mod fixed_base {
    use concord::api;

    api! {
        client ExampleApi {
            timeout 30s;
            base "https://api.example.test/v1/";
        }
    }
}

pub const DEFAULT_LABEL: Option<&'static str> = Some("documented");

pub mod values {
    use concord::api;

    api! {
        client ValuesApi {
            timeout 30s;
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
            timeout 30s;
            base "https://request-response.example.test/";


        POST update(
            id: String,
            body payload: UpdateInput via concord::Json,
        ) -> Item via Json;
    }}
}

pub mod identity {
    use concord::api;

    use super::Item;

    api! {
        client IdentityApi {
            timeout 30s;
            base "https://identity.example.test/";


        GET by_id(id: String) -> Item via Json {
            id "catalog.item.by-id";
        }
    }}
}

pub mod expressions {
    use concord::api;

    api! {
        client ExpressionApi {
            timeout 30s;
            base "https://expression.example.test/";


        scope regions(region: String) {
            GET item(id: String) -> () via Empty {
                path [concat ["item-", endpoint.id, "-", regions.region]];
            }
        }
    }}
}

pub mod shaping {
    use concord::api;

    api! {
        client ShapingApi {
            timeout 30s;
            base "https://shaping.example.test/";
            values {
                locale?: String;
            }


        scope regions(region: String) {
            GET item(id: String, tags: Vec<String>) -> () via Empty {
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
            timeout 30s;
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
    }}
}

pub mod timeouts {
    use concord::api;

    api! {
        client TimeoutApi {
            timeout 30s;
            base "https://timeout.example.test/";


        GET milliseconds() -> () via Empty { timeout 250ms; }
        GET seconds() -> () via Empty { timeout 5s; }
        GET minutes() -> () via Empty { timeout 10m; }
        GET hours() -> () via Empty { timeout 1h; }
        GET no_timeout() -> () via Empty { timeout 30s; }
    }}
}

pub mod pagination {
    use concord::api;

    api! {
        client PaginationApi {
            timeout 30s;
            base "https://pagination.example.test/";


        GET page(
            offset: u64,
            page_size: std::num::NonZeroU32,
        ) -> Vec<u32> via Json {
            query {
                "offset" = endpoint.offset;
                "page_size" = endpoint.page_size;
            }
            paginate via concord::pagination::OffsetPagination {
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
            timeout 30s;
            base "https://rate-limit.example.test/";
            values {
                tenant: String;
            }
            rate_limits {
                policy account_reads(account) {
                    id "service.account.read";
                    key [origin, account];
                    cost 1;
                    allow 100 per 1m;
                    allow 1_000 per 1h;
                }
            }


        scope accounts(account: String, region: String) {
            rate_limit account_reads(account = accounts.account) { cost 2; }

            GET item(item_id: String) -> () via Empty {
                rate_limit {
                    id "service.item.read";
                    key [origin, endpoint, client.tenant, accounts.region, endpoint.item_id];
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
            timeout 30s;
            base "https://example.test/";
            values {
                region: String;
            }
            subdomain replace [client.region, "api"];


        GET health() -> serde_json::Value via Json {
            path ["health"];
        }
    }}
}

pub mod quick_start_auth {
    use concord::api;

    api! {
        client PrivateApi {
            timeout 30s;
            base "https://private.example.test/";
            credentials {
                secret token;
                credential bearer_token = bearer(secret.token);
            }
            auth { bearer bearer_token; }


        GET account() -> serde_json::Value via Json;
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
                policy tenant_reads(tenant) { cost 1;
                    id "kitchen.tenant.read";
                    key [origin, tenant];
                    allow 100 per 1m;
                }
            }
            subdomain replace ["api"];
            path ["v1"];
            query { "locale" = client.locale; }
            headers { "X-Client" = "kitchen-sink"; }
            auth { header "X-Api-Key" = application_key; }
            timeout 10s;
            rate_limit { key [origin]; cost 1;
                id "kitchen.global";
                allow 1_000 per 1m;
            }


        GET token_endpoint() -> crate::authentication::extensions::ExampleToken via crate::authentication::extensions::ExampleTokenCodec;

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
            subdomain replace [regions.region, "api"];
            path ["regions", regions.region];
            headers { "X-Region" = regions.region; }
            rate_limit tenant_reads(tenant = client.tenant);

            POST search(
                category: String,
                offset: u64,
                page_size: std::num::NonZeroU32,
                body filter: UpdateInput via concord::Json,
            ) -> Vec<Item> via concord::Json {
                id "kitchen.regions.search";
                effect safe;
                path ["items"];
                query {
                    "category" = endpoint.category;
                    "offset" = endpoint.offset;
                    "page_size" = endpoint.page_size;
                }
                auth replace { bearer bearer challenge unauthorized; }
                timeout 5s;
                rate_limit {
                    id "kitchen.search";
                    key [endpoint, endpoint.category];
                    cost 2;
                    allow 100 per 1m;
                }
                paginate via concord::pagination::OffsetPagination {
                    offset -> endpoint.offset;
                    page_size -> endpoint.page_size;
                }
            }

            METHOD("PROPFIND") inspect(id: String) -> String via Text {
                path [concat ["item-", endpoint.id]];
                auth off;
                timeout 30s;
            }
        }
    }}
}
