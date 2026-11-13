concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(impl: String) { query { impl; } } }
}
fn main() {}
