concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { body make { "q" = 1; "q" = 2; } } }
}
fn main() {}
