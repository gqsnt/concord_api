concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { 200u16 => () } } }
}
fn main() {}
