concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(code: u32, code: u32) { query { code; } } }
}
fn main() {}
