struct Bad;
concord::api! { client C { base "https://span.test";
read Bad;
GET get() -> u32; } }
fn main() {}
