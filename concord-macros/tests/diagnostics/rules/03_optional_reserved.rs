concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(request?: String) { query { request; } } }
}
fn main() {}
