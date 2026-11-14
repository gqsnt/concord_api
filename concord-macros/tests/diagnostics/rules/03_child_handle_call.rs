concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope branch { GET call(); } }
}
fn main() {}
