concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(abstract: String) { query { abstract; } } }
}
fn main() {}
