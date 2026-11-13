concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { remove headers {} } }
}
fn main() {}
