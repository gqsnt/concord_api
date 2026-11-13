concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part one() {} GET fetch(..one,..one); }
}
fn main() {}
