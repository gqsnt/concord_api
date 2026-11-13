concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(await: String) { query { await; } } }
}
fn main() {}
