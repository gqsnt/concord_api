concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(client: String) { query { client; } } }
}
fn main() {}
