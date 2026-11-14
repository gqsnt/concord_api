struct Bad;
concord::api! { client C { base "https://span.test";
problem u32 read Bad;
GET get(); } }
fn main() {}
