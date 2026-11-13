concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { 2_00 => () } } }
}
fn main() {}
