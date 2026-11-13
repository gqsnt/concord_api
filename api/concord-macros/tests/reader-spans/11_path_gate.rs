concord::api! { client C(key:concord::Secret) { base "https://one.test";
path "{key}";
GET get() { base "https://two.test"; } } }
fn main() {}
