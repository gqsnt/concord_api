concord::api! { client C { base "https://span.test"; POST get() {
body 42;
}
PUT upload(payload?: concord::Body) {
body payload;
} } }
fn main() {}
