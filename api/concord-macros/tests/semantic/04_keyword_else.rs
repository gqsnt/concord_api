concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(else: String) { query { else; } } }
}
fn main() {}
