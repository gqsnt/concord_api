#![allow(non_camel_case_types)]
type r#json = concord::Text;
concord::api! { pub client A { base "http://127.0.0.1";
    POST p(self::r#json b: String);
} }
fn main() {}
