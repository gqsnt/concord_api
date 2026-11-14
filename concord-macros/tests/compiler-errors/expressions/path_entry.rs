concord::api! { client Leak(key:String) { base "https://path.test"; path "{key}"; GET get() { path "{key}"; } } }
fn main() {}
