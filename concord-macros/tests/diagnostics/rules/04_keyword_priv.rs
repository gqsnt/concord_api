concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(priv: String) { query { priv; } } }
}
fn main() {}
