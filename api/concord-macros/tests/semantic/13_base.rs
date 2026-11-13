concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(server?: Base) { base server; } }
}
fn main() {}
