#[derive(Clone)] struct NotSer;
concord::api! { client C { base "https://span.test"; GET get(value:NotSer) {
query { value; }
} } }
fn main() {}
