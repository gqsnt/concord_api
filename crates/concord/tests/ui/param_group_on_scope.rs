concord::api! {
    pub client C() {
        base "https://x.test";
        params Paging(query limit?: u32);
        scope items(..Paging) {
            GET list(..Paging) { path "items"; }
        }
    }
}

fn main() {}
