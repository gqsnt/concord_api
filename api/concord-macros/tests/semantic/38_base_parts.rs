concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { base "https://part.test"; } part two() { base "https://part.test"; } GET fetch(..one,..two); }
}
fn main() {}
