concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(match: String) { query { match; } } }
}
fn main() {}
