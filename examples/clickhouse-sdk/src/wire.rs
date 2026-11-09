//! The declaration: the four requests of this slice, as the ClickHouse HTTP interface documents
//! them. Private to the crate; [`ClickHouse`](crate::ClickHouse) is the public client.
//!
//! - `select`: `GET /?query=<sql>`. The HTTP interface treats every `GET` as read-only, so a
//!   statement that modifies data is refused by the server whatever the SQL text says.
//! - `insert`: `POST /?query=INSERT INTO .. FORMAT ..` with the rows as the body (the server joins
//!   the URL part and the body with a line feed).
//! - `command`: `POST /` with SQL in the body (here only `KILL QUERY`).
//! - `ping`: `GET /ping`.
//!
//! Query parameters (`param_<name>`) have names chosen per query; the validated pairs are
//! placed after declared query keys by `query(..)`.

use concord::reqwest::header::HeaderValue;
use concord::stream::Upload;
use concord::{Encoded, Text};

use crate::format::OutputFormat;
use crate::query::{Identifier, QueryId};

concord::api! {
    /// The ClickHouse HTTP interface of one server.
    pub client ClickHouseHttp(
        origin: concord::BaseUrl,
        /// `Authorization: Basic ..` (HTTP Basic authentication), flagged sensitive.
        #[sensitive] header("authorization") authorization?: HeaderValue,
        /// `X-ClickHouse-User` (header authentication); a credential like the key, so sensitive
        /// (Basic authentication withholds the user too).
        #[sensitive] header("x-clickhouse-user") user?: String,
        /// `X-ClickHouse-Key` (header authentication).
        #[sensitive] header("x-clickhouse-key") key?: String,
        /// The default database of the request (`X-ClickHouse-Database`).
        database?: Identifier,
        /// The quota key (`quota_key`, "any string").
        query("quota_key") quota_key?: String,
    ) {
        base origin;
        headers { "x-clickhouse-database" = "{database}"; }

        /// A read-only query; the response format is the `X-ClickHouse-Format` header.
        GET select(
            query("query") sql: String,
            format: OutputFormat,
            query("query_id") query_id?: QueryId,
            query("wait_end_of_query") wait_end_of_query?: u8,
            query("send_progress_in_http_headers") progress?: u8,
            query(..) params?: Vec<(String, String)>,
        ) -> stream {
            headers { "x-clickhouse-format" = "{format}"; }
        }

        /// An insert: the statement in the URL, the rows streamed as the body.
        POST insert(
            query("query") statement: String,
            query("query_id") query_id?: QueryId,
            body rows: Upload,
        ) -> stream {
            request Encoded;
        }

        /// A statement in the body (`KILL QUERY`), answered in `format`.
        POST command(
            body sql: String,
            format: OutputFormat,
            query("query_id") query_id?: QueryId,
            query(..) params?: Vec<(String, String)>,
        ) -> stream {
            request Text;
            headers { "x-clickhouse-format" = "{format}"; }
        }

        /// The health check: `Ok.` and a line feed.
        GET ping() -> String {
            path "ping";
            response Text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_parameters_are_hidden_by_endpoint_debug_but_shown_by_prepared_debug() {
        let endpoint = ClickHouseHttp::new(
            concord::reqwest::Client::new(),
            crate::wire::ClickHouseHttpConfig {
                origin: concord::BaseUrl::from_static("http://localhost:8123"),
            },
        )
        .select("SELECT {x:String}".to_owned(), OutputFormat::JsonEachRow)
        .set_params(vec![("param_x".to_owned(), "fixture-value".to_owned())]);
        assert!(!format!("{endpoint:?}").contains("fixture-value"));
        let prepared = endpoint.prepare().unwrap();
        assert!(format!("{prepared:?}").contains("param_x=fixture-value"));
    }
}
