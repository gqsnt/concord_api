concord::api! { client C { base "https://books.test"; read concord::Json; GET get() -> Result<u32, String> { status { 400 => Err } } } }
fn main() {}
