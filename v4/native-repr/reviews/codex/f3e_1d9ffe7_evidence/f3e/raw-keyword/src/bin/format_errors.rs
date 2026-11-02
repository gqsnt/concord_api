#![allow(non_camel_case_types, dead_code)]
pub mod format_body { type r#type = concord::Text; concord::api! { pub client A { base "http://127.0.0.1"; POST p(r#type b: String); } } }
pub mod format_output { type r#type = concord::Text; concord::api! { pub client A { base "http://127.0.0.1"; GET p() -> r#type String; } } }
fn main() {}
