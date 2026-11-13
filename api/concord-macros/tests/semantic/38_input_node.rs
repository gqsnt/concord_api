concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one(code: u32) { query { code; } } GET fetch(code: u32,..one) { query { code; } } }
}
fn main() {}
