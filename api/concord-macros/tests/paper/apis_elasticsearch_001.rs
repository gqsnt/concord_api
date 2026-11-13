concord::api! {
    pub client Elasticsearch(url: Base, bulk_url: Base, authorization: Secret) {
        base url;
        headers { "authorization" = authorization; }
        read Json;
        problem EsProblem;
        /// Logs endpoint names and try numbers, never payloads or credentials.
        middleware TraceRequests;

        part output(pretty?: bool, filter_path?: Vec<String>) {
            query { pretty; "filter_path" = filter_path.as_ref().map(|p| p.join(",")); }
        }
        part write_options(refresh?: Refresh, routing?: String) {
            query { refresh; routing; }
        }

        /// One index or single-index alias; not a wildcard or comma list.
        scope index(name: String) {
            path "{name}";
            PUT index_document(id: String, document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                body json(document);
                idempotent false;
            }
            POST index_auto(document: Value, ..output, ..write_options) -> WriteReply {
                path "_doc";
                body json(document);
            }
            GET get(id: String, ..output) -> GetReply {
                path "_doc/{id}";
                status { 404 { body } => document_404::<GetReply>(body)? }
            }
            HEAD exists(id: String, ..output) -> bool {
                path "_doc/{id}";
                status { 2xx => true, 404 => false }
            }
            /// Patch is an update envelope: doc, script, upsert, etc.
            POST update(id: String, patch: Value, ..output, ..write_options) -> WriteReply {
                path "_update/{id}";
                body json(patch);
            }
            DELETE delete_document(id: String, ..output, ..write_options) -> WriteReply {
                path "_doc/{id}";
                status { 404 { body } => document_404::<WriteReply>(body)? }
                idempotent false;
            }
            PUT create(settings: Value, ..output) -> IndexAck {
                body json(settings);
                idempotent false;
            }
            DELETE delete(..output) -> IndexAck { idempotent false; }
        }

        /// A comma list and/or wildcard expression, kept in one path segment.
        scope indices(targets: String) {
            path "{targets}";
            GET search(request: Value, ..output) -> SearchPage {
                path "_search";
                body json(request);
                idempotent;
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

        POST bulk(lines: Vec<Value>, ..output, ..write_options) -> BulkReply {
            base client.bulk_url;
            headers { override "authorization" = client.authorization; }
            path "_bulk";
            body ndjson(lines);
        }
        /// No index in the URL. Cursor replaces both PIT id and search_after.
        /// filter_path is deliberately unavailable: the pager needs its fields.
        POST search_pit(query: Value, pit_id: String, keep_alive: String,
                        size?: u32, cursor?: PitCursor, pretty?: bool) -> SearchPage {
            path "_search";
            query { pretty; }
            body json(pit_body(query, pit_id, keep_alive, size, cursor));
            paginate pit_after(cursor);
            idempotent;
        }
        DELETE close_pit(id: String, ..output) -> ClosedPit {
            path "_pit";
            body json(j!({ "id": id }));
            idempotent false;
        }
        GET cat_indices(format?: CatFormat, ..output) -> CatReply {
            path "_cat/indices";
            query { format; }
            read Text;
            status { 2xx { body } => cat_reply(body, format)? }
        }
    }
}
