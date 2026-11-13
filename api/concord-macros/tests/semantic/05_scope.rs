concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope Bad__Name { GET fetch(); } }
}
fn main() {}
