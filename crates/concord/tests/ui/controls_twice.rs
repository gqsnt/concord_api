concord::api! {
    pub client Api(key: String) {
        base "http://x.test";
        controls { remove set_key; }
        controls { pub(crate) with_base_url; }
        headers { "x-key" = key; }
        GET ping();
    }
}
fn main() {}
