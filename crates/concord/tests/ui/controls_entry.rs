// Every entry says what it does to the control.
concord::api! {
    pub client Api(key: String) {
        base "http://x.test";
        controls { set_key; }
        headers { "x-key" = key; }
        GET ping();
    }
}
fn main() {}
