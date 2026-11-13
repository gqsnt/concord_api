concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(extern: String) { query { extern; } } }
}
fn main() {}
