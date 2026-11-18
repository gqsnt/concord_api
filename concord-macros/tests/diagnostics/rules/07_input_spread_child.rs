concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part p(outer?: String) { query { outer; } } scope outer { scope g(..p) { GET outer_value(); } } }
}
fn main() {}
