concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { body make { remove "mode"; } } }
}
fn main() {}
