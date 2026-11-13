concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { override base "https://edits.test"; } }
}
fn main() {}
