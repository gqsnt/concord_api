concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(crate: String) { query { crate; } } }
}
fn main() {}
