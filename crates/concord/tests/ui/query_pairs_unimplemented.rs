concord::api! {
    pub client C() {
        base "https://x.test";
        GET list(query(..) page: u32) { path "items"; }
    }
}

fn main() {}
