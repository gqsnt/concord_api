concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(edit_request?: String) { query { edit_request; } } }
}
fn main() {}
