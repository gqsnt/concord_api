concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(return: String) { query { return; } } }
}
fn main() {}
