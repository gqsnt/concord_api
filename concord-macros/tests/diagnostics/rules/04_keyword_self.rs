concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(self: String) { query { self; } } }
}
fn main() {}
