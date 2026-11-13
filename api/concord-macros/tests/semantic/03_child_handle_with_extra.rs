concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope branch { GET with_extra(); } }
}
fn main() {}
