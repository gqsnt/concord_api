concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; status { 4xx => () } GET fetch() { status { 410 => () } } }
}
fn main() {}
