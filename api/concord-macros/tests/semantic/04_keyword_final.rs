concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(final: String) { query { final; } } }
}
fn main() {}
