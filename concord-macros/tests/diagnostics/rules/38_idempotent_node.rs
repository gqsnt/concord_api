concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { idempotent; } GET fetch(..one) { idempotent; } }
}
fn main() {}
