concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { query { missing_code; } } }
}
fn main() {}
