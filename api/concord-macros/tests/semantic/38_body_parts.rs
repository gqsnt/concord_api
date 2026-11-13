concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { body make(); } part two() { body make(); } GET fetch(..one,..two); }
}
fn main() {}
