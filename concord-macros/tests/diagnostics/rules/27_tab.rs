concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { path "record\tdata"; } }
}
fn main() {}
