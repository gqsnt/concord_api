concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { 201 { headers, headers: other } => () } } }
}
fn main() {}
