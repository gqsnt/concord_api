extern crate c4 as codec;
c4::api! { pub client A { base "http://127.0.0.1"; POST p(::codec::Text b: String); } }
fn main() {}
