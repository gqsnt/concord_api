concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope outer { GET fetch(outer: String) { query { outer; } } } }
}
fn main() {}
