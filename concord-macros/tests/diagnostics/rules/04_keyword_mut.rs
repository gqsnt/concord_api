concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(mut: String) { query { mut; } } }
}
fn main() {}
