concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(send_raw?: String) { query { send_raw; } } }
}
fn main() {}
