// A control is public unless listed: `pub` says nothing.
concord::api! {
    pub client Api(key: String) {
        base "http://x.test";
        controls { pub set_key; }
        headers { "x-key" = key; }
        GET ping();
    }
}
fn main() {}
