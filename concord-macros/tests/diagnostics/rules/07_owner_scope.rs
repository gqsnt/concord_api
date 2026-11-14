concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope client { GET fetch(); } }
}
fn main() {}
