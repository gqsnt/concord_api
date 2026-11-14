concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(use: String) { query { use; } } }
}
fn main() {}
