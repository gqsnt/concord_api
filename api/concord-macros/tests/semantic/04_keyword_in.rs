concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(in: String) { query { in; } } }
}
fn main() {}
