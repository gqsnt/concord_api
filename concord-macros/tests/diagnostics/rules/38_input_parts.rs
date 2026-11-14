concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one(code: u32) { query { code; } } part two(code: u32) { query { code; } } GET fetch(..one,..two); }
}
fn main() {}
