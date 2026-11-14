concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(send_full?: String) { query { send_full; } } }
}
fn main() {}
