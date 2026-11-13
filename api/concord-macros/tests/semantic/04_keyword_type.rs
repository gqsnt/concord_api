concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(type: String) { query { type; } } }
}
fn main() {}
