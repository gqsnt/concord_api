concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { 099 => () } } }
}
fn main() {}
