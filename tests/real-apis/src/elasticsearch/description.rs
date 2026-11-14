use concord::{json, Base, Json, Secret, Text};
use serde_json::Value;

concord::api! {
    /// Elasticsearch: documents, searches, a point in time, bulk writes, and the `_cat` tables.
    pub client Elasticsearch(url: Base, authorization: Secret) {
        base url;
        headers { "authorization" = authorization; }
        read Json;
        problem EsProblem;

        part output(pretty?: bool, filter_path?: Vec<String>) {
            query { pretty; "filter_path" = filter_path.as_ref().map(|p| p.join(",")); }
        }
        part write_options(refresh?: Refresh, routing?: String) {
            query { refresh; routing; }
        }

        /// One index, or an alias of one index: not a wildcard, not a list.
        scope index(name: String) {
            path "{name}";

            /// Writing a document again makes a new version of it: not sent twice.
            PUT put(id: String, document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                body json(document);
                idempotent false;
            }
            POST add(document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc";
                body json(document);
            }
            /// A missing document is a normal response (`found: false`); a missing index is an error.
            GET get(id: String, ..output) -> GetReply {
                path "_doc/{id}";
                status { 404 { body } => document_404(body)? }
            }
            HEAD exists(id: String) -> bool {
                path "_doc/{id}";
                status { 2xx => true, 404 => false }
            }
            /// `changes` is Elasticsearch's own envelope: `doc`, `script`, `upsert`.
            POST update(id: String, changes: Value, ..output, ..write_options) -> WriteReply {
                path "_update/{id}";
                body json(changes);
            }
            DELETE delete_document(id: String, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                status { 404 { body } => document_404(body)? }
                idempotent false;
            }
            PUT create(settings: Value, ..output) -> IndexAck {
                body json(settings);
                idempotent false;
            }
            DELETE delete(..output) -> IndexAck { idempotent false; }
        }

        /// Several indices: a list and wildcards in one segment, `logs-*,metrics-*`.
        scope indices(targets: String) {
            path "{targets}";

            /// A `GET` with a body. `.post()` sends the same search as a `POST`.
            GET search(request: Value, ..output) -> SearchPage {
                path "_search";
                body json(request);
            }
            GET count(request: Value, ..output) -> CountReply {
                path "_count";
                body json(request);
            }
            POST open_pit(keep_alive: String, ..output) -> OpenedPit {
                path "_pit";
                query { keep_alive; }
            }
        }

        /// Many writes in one request: a line of action, then a line of document, for each one.
        POST bulk(lines: Vec<Value>, ..output, ..write_options) -> BulkReply {
            path "_bulk";
            body ndjson(lines);
        }

        /// A search in a point in time, page after page. No index in the URL: the point in time
        /// has them. No `filter_path` either: the pager needs every field it reads.
        POST search_pit(query: Value, pit: Pit, size?: u32, search_after?: Vec<Value>) -> SearchPage {
            path "_search";
            body json {
                query; pit; size; search_after;
                "sort" = ["_shard_doc"];
                "track_total_hits" = false;
            }
            paginate pit_after(pit, search_after);
            idempotent;
        }
        DELETE close_pit(id: String) -> ClosedPit {
            path "_pit";
            body json { id; }
        }

        /// A text table, or JSON rows with `.format(CatFormat::Json)`.
        GET cat_indices(format?: CatFormat) -> CatReply {
            path "_cat/indices";
            query { format; }
            read Text;
            status { 2xx { body } => cat_reply(body, format)? }
        }
    }
}
