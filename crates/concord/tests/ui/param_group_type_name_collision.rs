concord::api! {
    pub client C() {
        base "https://x.test";
        params CList(query limit?: u32);
        GET list(..CList) { path "items"; }
    }
}

fn main() {}
