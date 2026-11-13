concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { status { 410 => () } } part two() { status { 410 => () } } GET fetch(..one,..two); }
}
fn main() {}
