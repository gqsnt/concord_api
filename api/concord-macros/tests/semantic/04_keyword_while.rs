concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(while: String) { query { while; } } }
}
fn main() {}
