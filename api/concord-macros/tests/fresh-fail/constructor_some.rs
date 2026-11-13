concord::api! { client C { base "https://books.test"; read concord::Json; GET get() -> Option<u32> { status { 404 => None, 200 => Some } } } }
fn main() {}
