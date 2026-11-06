concord::api! {
    pub client C() {
        base "https://x.test";
        scope items {
            params Paging(query limit?: u32);
            GET list(..Paging) { path "items"; }
        }
    }
}

fn main() {}
