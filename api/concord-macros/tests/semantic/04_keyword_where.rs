concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(where: String) { query { where; } } }
}
fn main() {}
