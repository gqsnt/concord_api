concord::api! { client C { base "https://span.test"; read concord::Json;
GET get() -> u32 {
status { 404 { body } => { let _ = &body; 17 } }
} } }
fn main() {}
