concord::api! { client C { base "https://span.test"; GET get() {
idle_timeout 42;
} } }
fn main() {}
