concord::api! { client C { base "https://span.test"; GET get() -> u32 {
status { 404 => "wrong" }
} } }
fn main() {}
