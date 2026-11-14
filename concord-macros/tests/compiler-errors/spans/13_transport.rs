concord::api! { client C { base "https://span.test"; GET get(); } }
fn caller(){ let _=C::new().with_transport(42); }
fn main() {}
