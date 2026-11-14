concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { override query {} } }
}
fn main() {}
