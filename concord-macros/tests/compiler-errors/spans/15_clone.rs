struct NotClone;
concord::api! { client C { base "https://span.test"; GET get(value:NotClone) {
query { "key" = { let _ = value; 42 }; }
} } }
fn main() {}
