concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { idle_timeout clock(); } GET fetch(..one) { idle_timeout clock(); } }
}
fn main() {}
