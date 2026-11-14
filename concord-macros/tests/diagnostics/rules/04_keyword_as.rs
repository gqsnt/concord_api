concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(as: String) { query { as; } } }
}
fn main() {}
