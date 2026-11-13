concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(r#loop: String) { query { r#loop; } } }
}
fn main() {}
