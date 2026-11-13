concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { remove query {} } }
}
fn main() {}
