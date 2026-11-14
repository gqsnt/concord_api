concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope error { GET fetch(); } }
}
fn main() {}
