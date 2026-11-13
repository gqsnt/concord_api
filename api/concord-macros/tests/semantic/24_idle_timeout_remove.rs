concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { remove idle_timeout clock(); } }
}
fn main() {}
