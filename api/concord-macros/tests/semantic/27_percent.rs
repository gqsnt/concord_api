concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { path "record%20data"; } }
}
fn main() {}
