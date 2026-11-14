concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(box: String) { query { box; } } }
}
fn main() {}
