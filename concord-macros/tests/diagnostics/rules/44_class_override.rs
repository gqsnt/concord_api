concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; status { 202 => () } GET fetch() { status { override 2xx => () } } }
}
fn main() {}
