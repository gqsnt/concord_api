concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { headers { "x-q" = 1; "x-q" = 2; } } }
}
fn main() {}
