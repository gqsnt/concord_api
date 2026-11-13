concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(label: &str) { query { label; } } }
}
fn main() {}
