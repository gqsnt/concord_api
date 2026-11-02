#![allow(non_camel_case_types)]
type r#type = concord::Text;
concord::api! { pub client A { base "http://127.0.0.1";
    POST p(self::type b: String);
} }
fn main() {}
