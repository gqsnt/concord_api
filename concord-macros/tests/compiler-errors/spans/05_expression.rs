#[derive(Clone)] struct NotSer;
concord::api! { client C { base "https://span.test"; GET get() {
query { "key" = NotSer; }
} } }
fn main() {}
