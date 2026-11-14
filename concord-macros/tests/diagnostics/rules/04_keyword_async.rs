concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(async: String) { query { async; } } }
}
fn main() {}
