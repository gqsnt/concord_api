concord::api! { client C { base "https://span.test"; POST get() {
body 42;
} } }
fn main() {}
