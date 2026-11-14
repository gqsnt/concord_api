concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { override idle_timeout clock(); } }
}
fn main() {}
