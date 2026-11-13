concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(break: String) { query { break; } } }
}
fn main() {}
