concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope outer { POST post(outer?: String) { query { outer; "other" = outer; } headers { outer; } body json { outer; } } } }
}
fn main() {}
