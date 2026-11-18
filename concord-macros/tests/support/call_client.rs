concord::api! {
    pub client Gate {
        base "https://gate.example";
        read concord::Json;
        GET plain(id: u32, only_plain?: u32) { query { id; only_plain; } }
        GET paged(cursor?: u32) -> Vec<u32> {
            query { cursor; }
            paginate concord::by_offset(cursor);
        }
        POST post();
        DELETE delete();
    }
}
