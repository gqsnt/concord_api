concord::api! { client C { base "https://span.test"; POST get() {
idempotent "yes";
} } }
fn main() {}
