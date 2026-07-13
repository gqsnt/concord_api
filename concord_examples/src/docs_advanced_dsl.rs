use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedUser {
    pub id: u64,
}

#[derive(Default)]
pub struct AdvancedRateLimitHeaders;

impl RateLimitObserver for AdvancedRateLimitHeaders {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        ctx.on_429()
            .scope_header("x-rate-limit-scope")
            .retry_after()
    }
}

api! {
    client DocsAdvancedDslApi {
        base "https://api.example.com"

        auth {
            secret username: String
            secret password: String
            secret query_key: String
            secret client_id: String
            secret client_secret: String

            credential basic_login = basic(secret.username, secret.password)
            credential query_key = api_key(secret.query_key)
            credential oauth_session = oauth2_client {
                token_url: "https://auth.example.com/oauth/token",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
                scope: "read:users",
            }
        }

        policies {
            rate_limit tenant {
                bucket method by [host, endpoint, method, "tenant", tenant_key] {
                    cost 2
                    10 / 1s
                }
            }

            observe rate_limit AdvancedRateLimitHeaders
        }

        profiles {
            profile basic_write {
                auth basic basic_login
            }

            profile tenant_read {
                auth bearer oauth_session
                rate_limit tenant
            }

            profile query_authenticated {
                auth query "api_key" = query_key
            }
        }

    }

    scope tenants(tenant_id: String) {
        path ["tenants", tenant_id]
        rate_limit key tenant_key = tenant_id

        GET ListUsers(request_id: String)
        path ["users"]
        header "X-Request-Id" = request_id,
        query "tenant" = tenant_id,
        rate_limit only tenant
        profile tenant_read
        -> Json<Vec<AdvancedUser>>

        GET SearchTaggedUsers(request_id: String, tags: Vec<String>)
        path ["users", "tagged"]
        header "X-Request-Id" = request_id,
        header "X-Debug" -
        query "tag" = tags,
        query "debug" -
        timeout: std::time::Duration::from_secs(5),
        profile tenant_read
        -> Json<Vec<AdvancedUser>>

        POST CreateUser(body: Json<AdvancedUser>)
        path ["users"]
        profile basic_write
        -> Json<AdvancedUser>
    }

    GET QueryAuthenticated
    path ["query-authenticated"]
    profile query_authenticated
    -> Json<AdvancedUser>
}
