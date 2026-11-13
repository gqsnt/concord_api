concord::api! { client C { base "https://span.test"; read concord::Json; GET get() -> Vec<u32> {
paginate 42;
} } }
fn main() {}
