concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope outer { scope outer { GET fetch(); } } }
}
fn main() {}
