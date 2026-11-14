concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(continue: String) { query { continue; } } }
}
fn main() {}
