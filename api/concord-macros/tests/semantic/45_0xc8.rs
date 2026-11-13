concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { 0xc8 => () } } }
}
fn main() {}
