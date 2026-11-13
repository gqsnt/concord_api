concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(trait: String) { query { trait; } } }
}
fn main() {}
