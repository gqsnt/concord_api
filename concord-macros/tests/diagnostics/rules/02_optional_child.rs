concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope records(fetch?: u8) { query { fetch; } GET fetch(); } }
}
fn main() {}
