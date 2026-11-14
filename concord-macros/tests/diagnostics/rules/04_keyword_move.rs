concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(move: String) { query { move; } } }
}
fn main() {}
