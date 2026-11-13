concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(#[cfg(feature="extra")] code: u32) { query { code; } } }
}
fn main() {}
