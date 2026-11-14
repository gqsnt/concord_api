concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { read Reader; } GET fetch(..one) { read Reader; } }
}
fn main() {}
