concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(static: String) { query { static; } } }
}
fn main() {}
