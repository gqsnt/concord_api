concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { read Reader; } part two() { read Reader; } GET fetch(..one,..two); }
}
fn main() {}
