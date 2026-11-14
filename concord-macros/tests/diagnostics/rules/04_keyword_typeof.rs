concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(typeof: String) { query { typeof; } } }
}
fn main() {}
