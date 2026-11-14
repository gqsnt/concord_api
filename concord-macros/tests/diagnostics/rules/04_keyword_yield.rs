concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(yield: String) { query { yield; } } }
}
fn main() {}
