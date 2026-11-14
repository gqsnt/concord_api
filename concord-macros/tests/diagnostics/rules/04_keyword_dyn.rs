concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(dyn: String) { query { dyn; } } }
}
fn main() {}
