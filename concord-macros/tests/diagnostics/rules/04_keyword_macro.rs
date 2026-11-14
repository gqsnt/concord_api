concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(macro: String) { query { macro; } } }
}
fn main() {}
