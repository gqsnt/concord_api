concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { headers { "x-q" = 1; } } part two() { headers { "x-q" = 1; } } GET fetch(..one,..two); }
}
fn main() {}
