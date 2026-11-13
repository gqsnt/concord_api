concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; query { "code" = 1; } GET fetch(code: u32) { query { code; } } }
}
fn main() {}
