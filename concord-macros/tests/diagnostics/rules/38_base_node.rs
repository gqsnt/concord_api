concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { base "https://part.test"; } GET fetch(..one) { base "https://part.test"; } }
}
fn main() {}
