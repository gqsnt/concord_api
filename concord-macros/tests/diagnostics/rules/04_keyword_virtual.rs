concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(virtual: String) { query { virtual; } } }
}
fn main() {}
