concord::api! { client C { base "https://span.test"; GET get(value:u32) { query {value;} } } }
fn caller(){ let _=C::new().get("wrong"); }
fn main() {}
