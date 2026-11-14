concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(#[deprecated] code: u32) { query { code; } } }
}
fn main() {}
