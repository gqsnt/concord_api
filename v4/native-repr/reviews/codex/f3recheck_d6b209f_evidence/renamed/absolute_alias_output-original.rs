extern crate c4 as codec;
c4::api! { pub client A { base "http://127.0.0.1"; GET p() -> String via ::codec::Text; } }
fn main() {}
