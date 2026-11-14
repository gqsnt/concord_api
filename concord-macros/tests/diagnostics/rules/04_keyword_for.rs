concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(for: String) { query { for; } } }
}
fn main() {}
