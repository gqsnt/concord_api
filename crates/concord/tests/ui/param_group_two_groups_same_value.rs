concord::api! {
    pub client C() {
        base "https://x.test";
        params A(query x?: u32);
        params B(query x?: u32, query y?: u32);
        GET list(..A, ..B) { path "items"; }
    }
}

fn main() {}
