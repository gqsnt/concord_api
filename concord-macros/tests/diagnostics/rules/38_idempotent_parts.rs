concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { idempotent; } part two() { idempotent; } GET fetch(..one,..two); }
}
fn main() {}
