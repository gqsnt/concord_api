concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { body make { "q" = 1; } } GET fetch(..one) { body make { "q" = 1; } } }
}
fn main() {}
