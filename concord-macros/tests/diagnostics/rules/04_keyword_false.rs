concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(false: String) { query { false; } } }
}
fn main() {}
