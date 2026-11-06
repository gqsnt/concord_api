concord::api! {
    pub client C() {
        base "https://x.test";
        params Paging(query limit?: u32, query cursor?: String);
        GET list(query limit?: u32, ..Paging) { path "items"; }
    }
}

fn main() {}
