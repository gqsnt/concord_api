concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(become: String) { query { become; } } }
}
fn main() {}
