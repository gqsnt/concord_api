concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(send_if_none_match?: String) { query { send_if_none_match; } } }
}
fn main() {}
