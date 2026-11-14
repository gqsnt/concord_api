concord::api! { client C { base "https://books.test"; read concord::Json; GET get() -> Result<u32, String> { status { 200 => Ok } } } }
fn main() {}
