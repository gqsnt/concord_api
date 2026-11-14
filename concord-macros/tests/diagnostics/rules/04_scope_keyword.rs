concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope r#mod { GET fetch(); } }
}
fn main() {}
