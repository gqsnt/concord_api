concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(at_url?: String) { query { at_url; } } }
}
fn main() {}
