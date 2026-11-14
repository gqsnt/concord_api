concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { query { "q" = 1; "q" = 2; } } }
}
fn main() {}
