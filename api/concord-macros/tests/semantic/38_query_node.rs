concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { query { "q" = 1; } } GET fetch(..one) { query { "q" = 1; } } }
}
fn main() {}
