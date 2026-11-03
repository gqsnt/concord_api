#![allow(non_camel_case_types)]
type r#type = concord::Text;
concord::api! { pub client A { base "http://127.0.0.1";
    POST p(body b: String via r#type);
    GET q() -> String via r#type;
} }
fn main() {}
