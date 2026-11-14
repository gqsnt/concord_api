concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(try: String) { query { try; } } }
}
fn main() {}
