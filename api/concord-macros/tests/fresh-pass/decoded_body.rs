concord::api! { pub client C { base "https://books.test"; read concord::Json;
    GET implicit() -> Option<u32> { status {404 => None} }
    GET explicit() -> Option<u32> { status {404 => None, 200 {body} => Some(body)} }
} }
fn main() { C::new().implicit().request().unwrap(); C::new().explicit().request().unwrap(); }
