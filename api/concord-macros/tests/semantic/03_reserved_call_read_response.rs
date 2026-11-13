concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(read_response?: String) { query { read_response; } } }
}
fn main() {}
