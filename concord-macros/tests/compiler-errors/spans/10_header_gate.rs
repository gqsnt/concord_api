concord::api! { client C(key:concord::Secret) { base "https://one.test";
headers { "x-key" = key; }
GET get() { base "https://two.test"; } } }
fn main() {}
