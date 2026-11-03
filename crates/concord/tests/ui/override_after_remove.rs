concord::api! {
    pub client Api(x?: String) {
        base "http://x.test";
        query { "k" = x; }
        scope s { query { remove "k"; } GET list(y?: String) { query { override "k" = y; } } }
    }
}
fn main() {}
