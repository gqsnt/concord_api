concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(struct: String) { query { struct; } } }
}
fn main() {}
