#[derive(Clone)] struct NotSer;
concord::api! { client C { base "https://span.test"; POST get(value:NotSer) {
body concord::json { value; }
} } }
fn main() {}
