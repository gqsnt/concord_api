concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(#[doc(hidden)] code: u32) { query { code; } } }
}
fn main() {}
