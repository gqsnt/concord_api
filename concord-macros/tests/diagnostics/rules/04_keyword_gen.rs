concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(gen: String) { query { gen; } } }
}
fn main() {}
