concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(const: String) { query { const; } } }
}
fn main() {}
