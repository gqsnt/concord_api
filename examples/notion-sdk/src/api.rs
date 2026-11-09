//! Concord declaration. The client wrapper adds domain errors and pagination.
//! Direct use retains Concord's ordinary prepare/send/receive and native escapes.

use crate::models::{Block, List, Page, QueryResult};
use crate::params::QueryBody;
use concord::Json;
use concord::reqwest::header::AUTHORIZATION;

concord::api! {
    /// The three Notion read operations in this SDK slice.
    pub client Notion(
        /// Bearer token, redacted from generated diagnostics.
        #[sensitive] token: String,
        /// Per-client version; changing a clone does not mutate another client's context.
        header("Notion-Version") version: String = crate::NOTION_VERSION.to_owned(),
    ) {
        base "https://api.notion.com/v1";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;
        limit crate::RESPONSE_LIMIT;

        /// Query a data source. Its cursor belongs in the JSON body.
        #[read] POST query(data_source_id: String, body query: QueryBody) -> List<QueryResult> {
            path "data_sources/{data_source_id}/query";
        }
        /// Retrieve properties and metadata; this does not retrieve page content.
        GET page(page_id: String) -> Page { path "pages/{page_id}"; }
        /// One level of children, with the cursor in the query string.
        GET children(block_id: String, query page_size: u8, query start_cursor?: String) -> List<Block> {
            path "blocks/{block_id}/children";
        }
    }
}

#[cfg(test)]
mod read_operation_tests {
    use super::*;
    #[test]
    fn declared_post_reads_keep_their_method_and_effect() {
        let operation = NotionQuery::operation();
        assert_eq!(operation.method(), Some("POST"));
        assert!(operation.is_read(), "{}", operation.name());
    }
}
