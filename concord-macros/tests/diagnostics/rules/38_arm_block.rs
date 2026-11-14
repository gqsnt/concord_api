concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { 410 => (), 410 => () } } }
}
fn main() {}
