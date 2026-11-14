concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; status { 410 => () } GET fetch() { status { 410 => () } } }
}
fn main() {}
