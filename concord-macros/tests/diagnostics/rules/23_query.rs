concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { query { remove "mode"; } } }
}
fn main() {}
