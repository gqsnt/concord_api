concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope outer { GET fetch(outer: String) { headers { outer; } } } }
}
fn main() {}
