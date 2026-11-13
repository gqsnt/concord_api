concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(ref: String) { query { ref; } } }
}
fn main() {}
