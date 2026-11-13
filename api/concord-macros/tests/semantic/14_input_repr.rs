concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(#[repr(C)] code: u32) { query { code; } } }
}
fn main() {}
