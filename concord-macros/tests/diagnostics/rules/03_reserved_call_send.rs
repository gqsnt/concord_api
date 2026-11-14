concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(send?: String) { query { send; } } }
}
fn main() {}
