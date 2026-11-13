concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(mod: String) { query { mod; } } }
}
fn main() {}
