concord::api! { client C { base "https://settings.test"; GET get() { retry concord::Retry::standard(); } } }
fn main() {}
