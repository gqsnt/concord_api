concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { override 410 => () } } }
}
fn main() {}
