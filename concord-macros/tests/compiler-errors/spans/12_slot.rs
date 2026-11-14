concord::api! { client C { base "https://span.test"; read concord::Json; GET get() -> Vec<u32> {
paginate concord::by_offset(42u32);
} } }
fn main() {}
