concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(do: String) { query { do; } } }
}
fn main() {}
