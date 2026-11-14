concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(if: String) { query { if; } } }
}
fn main() {}
