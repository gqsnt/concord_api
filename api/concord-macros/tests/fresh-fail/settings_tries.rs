concord::api! { client C { base "https://settings.test"; GET get() { tries 3; } } }
fn main() {}
