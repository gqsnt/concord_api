#[derive(Clone)] struct NotSer;
concord::api! { client C { base "https://span.test"; GET get(value:NotSer) {
path "{value}";
} } }
fn main() {}
