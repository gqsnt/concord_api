concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { body make { "a" = 1; } } POST fetch(..one) { body other { "b" = 2; } } }
}
fn main() {}
