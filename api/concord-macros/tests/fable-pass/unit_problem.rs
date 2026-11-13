concord::api! { client C { base "https://c.example"; read concord::Json; problem (); GET g(); } }
fn main() { let _ = C::new().g().request().unwrap(); }
