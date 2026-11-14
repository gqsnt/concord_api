concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { body make { "q" = 1; } } part two() { body make { "q" = 1; } } GET fetch(..one,..two); }
}
fn main() {}
