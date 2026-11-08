// `controls` is an owner statement: it precedes the child declarations, once.
concord::api! {
    pub client Api(key: String) {
        base "http://x.test";
        headers { "x-key" = key; }
        GET ping();
        controls { remove set_key; }
    }
}
fn main() {}
