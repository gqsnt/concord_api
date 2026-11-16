use concord::{error::Class, Retry};
use std::{io, time::Duration};

concord::api! {
    pub client Algolia(origin: concord::Base, application: String, key: concord::Secret) {
        base origin;
        headers { "x-algolia-application-id" = application; "x-algolia-api-key" = key; }
        POST search() -> serde_json::Value {
            path "1/indexes/books/query";
            body concord::json(serde_json::json!({"query": "rust"}));
            idempotent; read concord::Json;
        }
    }
}
concord::api! {
    pub client Elasticsearch(origin: concord::Base, authorization: concord::Secret) {
        base origin;
        headers { "authorization" = authorization; }
        GET search() -> serde_json::Value { path "books/_search"; read concord::Json; }
    }
}

/// Application errors retain the last call failure or the overall timeout.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn next_host(error: &concord::Error) -> bool {
    error.class() == Some(Class::Connect)
        || matches!(
            error.status().map(|status| status.as_u16()),
            Some(502..=504)
        )
}

/// Repeats this read over separately authorized clients, within one overall timeout.
pub async fn algolia(clients: &[Algolia], budget: Duration) -> Result<serde_json::Value, BoxError> {
    tokio::time::timeout(budget, async {
        let mut last: BoxError = io::Error::new(
            io::ErrorKind::InvalidInput,
            "No Algolia host was configured.",
        )
        .into();
        for host in clients {
            match host.search().with_retry(Retry::none()).await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    let again = next_host(&error);
                    last = error.into();
                    if !again {
                        break;
                    }
                }
            }
        }
        Err(last)
    })
    .await?
}

/// Repeats an Elasticsearch search; it does not accept a document write.
pub async fn elasticsearch(
    clients: &[Elasticsearch],
    budget: Duration,
) -> Result<serde_json::Value, BoxError> {
    tokio::time::timeout(budget, async {
        let mut last: BoxError = io::Error::new(
            io::ErrorKind::InvalidInput,
            "No Elasticsearch host was configured.",
        )
        .into();
        for host in clients {
            match host.search().with_retry(Retry::none()).await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    let again = next_host(&error);
                    last = error.into();
                    if !again {
                        break;
                    }
                }
            }
        }
        Err(last)
    })
    .await?
}
