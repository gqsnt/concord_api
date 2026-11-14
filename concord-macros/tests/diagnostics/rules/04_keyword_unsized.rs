concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(unsized: String) { query { unsized; } } }
}
fn main() {}
