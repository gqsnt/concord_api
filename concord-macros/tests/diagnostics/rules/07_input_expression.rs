concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope outer { GET get(outer: String) { query { "k" = String::len(outer); } } } }
}
fn main() {}
