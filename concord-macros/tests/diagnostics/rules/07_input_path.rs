concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope outer { GET get(outer: String) { path "items/{outer}"; } } }
}
fn main() {}
