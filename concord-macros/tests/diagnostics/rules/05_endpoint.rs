concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch__record(); }
}
fn main() {}
