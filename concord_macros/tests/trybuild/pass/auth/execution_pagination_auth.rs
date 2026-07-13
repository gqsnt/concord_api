use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest { username: String }
#[derive(Debug, Serialize, Deserialize)]
pub struct User;
use self::usage_execution_api::UsageExecutionApi;

api! {
    client UsageExecutionApi {
        base "https://example.com"
        secret upstream_key: String
        credential upstream = api_key(secret.upstream_key)
        credential session = endpoint auth_api::LoginForSession
    }

    scope auth_api {
        POST LoginForSession(body: Json<LoginRequest>)
            as login_for_session
            path ["login"]
            auth header "X-Upstream-Key" = upstream
            -> Json<AccessToken>
    }

    scope protected {
        auth bearer session

        GET Me
            as me
            path ["me"]
            -> Json<User>
    }

    GET Health
        as health
        path ["health"]
        -> Text<String>

    GET List(start: u64 = 0, count: u64 = 20)
        as list
        path ["items"]
        query {
            start
            count
        }
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Json<Vec<String>>
}

async fn execution_usage(api: UsageExecutionApi) -> Result<(), ApiClientError> {
    let _awaited = api.list().await?;
    let _value = api.list().count(100).execute().await?;
    let _decoded = api
        .health()
        .debug_level(DebugLevel::V)
        .timeout(Duration::from_secs(2))
        .clear_timeout()
        .inherit_timeout()
        .response()
        .await?;
    #[cfg(feature = "dangerous-raw-response")]
    let _raw = api.list().execute_raw_response().await?;
    #[cfg(not(feature = "dangerous-raw-response"))]
    let _decoded = api.list().execute().await?;
    let _items = api
        .list()
        .count(100)
        .paginate(PaginationTermination::hard_item_cap(10))
        .collect()
        .await?;
    let _pages = api
        .list()
        .count(100)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await?;

    api.auth_api()
        .login_for_session(LoginRequest { username: "a".to_string() })
        .acquire_as_session()
        .await?;
    let _me = api.protected().me().await?;

    let _advanced = api
        .request(
            usage_execution_api::endpoints::List::new()
                .count(10)
                .count_opt(Some(20))
                .count_opt(None)
                .clear_count(),
        )
        .execute()
        .await?;

    Ok(())
}

fn main() {}
