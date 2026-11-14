concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(pub: String) { query { pub; } } }
}
fn main() {}
