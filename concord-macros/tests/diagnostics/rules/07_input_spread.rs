concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part x(outer?: String) {} scope outer { GET get(..x) { query { outer; } } } }
}
fn main() {}
