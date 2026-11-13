concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() { status { 410 => () } } GET fetch(..one) { status { 410 => () } } }
}
fn main() {}
