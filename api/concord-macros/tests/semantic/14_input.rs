concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(#[serde(default)] code: u32) { query { code; } } }
}
fn main() {}
