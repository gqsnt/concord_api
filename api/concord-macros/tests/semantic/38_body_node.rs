concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { body make(); } GET fetch(..one) { body make(); } }
}
fn main() {}
