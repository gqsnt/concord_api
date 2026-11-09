// Only crate visibility or removal: `pub(super)` is not a control visibility.
concord::api! {
    pub client Api(key: String) {
        base "http://x.test";
        controls { pub(super) set_key; }
        headers { "x-key" = key; }
        GET ping();
    }
}
fn main() {}
