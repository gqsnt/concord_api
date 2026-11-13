concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(unsafe: String) { query { unsafe; } } }
}
fn main() {}
