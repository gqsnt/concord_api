concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { path "record\x7fdata"; } }
}
fn main() {}
