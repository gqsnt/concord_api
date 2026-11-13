concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { status { remove 410 } } }
}
fn main() {}
