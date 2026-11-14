concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(into_request?: String) { query { into_request; } } }
}
fn main() {}
