// The v3 declaration measured by `src/bin/head_cost.rs` (V3-O07).
//
// Keep the workload stable across measured V3 candidates: the same base
// URLs, owners, routes, query/header program, credential placement, rate
// policy and caller-visible construction calls. Required inputs remain
// positional, the optional `page` stays absent, and `count` is initialized
// to 25.

use concord::{Binary, Bytes, api};

api! {
    /// Planning client: a root query value and header, a scope path, a query
    /// program with required, initialized, absent optional and repeated
    /// values, one bearer placement and one rate policy.
    client HeadApi {
        base "https://head-cost.example.test/v1/";
        timeout 30s;

        values {
            query tenant: String;
        }

        credentials {
            secret api_secret;
            credential application_key = bearer(secret.api_secret);
        }

        headers { "X-Client-Version" = "3"; }

        scope records(collection: String) {
            path "records/{collection}";

            GET show(id: String) -> Bytes via Binary {
                path "{id}";
            }

            GET search(
                query term: String,
                query count: u32 = 25,
                query page?: u32,
                query("tag") each tags: Vec<String>,
            ) -> Bytes via Binary {
                path "search";
            }
        }

        GET protected() -> Bytes via Binary {
            path "protected";
            auth { bearer application_key; }
        }

        GET limited() -> Bytes via Binary {
            path "limited";
            rate_limit {
                id "concord.head_cost.1";
                key [origin];
                cost 1;
                allow 1000000 per 1s;
            }
        }
    }
}

api! {
    /// Execution client: one credential-free endpoint on a loopback base.
    client ExecApi {
        base "http://head-cost.example.test/";
        timeout 30s;

        GET bounded() -> Bytes via Binary {
            path "bounded";
        }
    }
}
