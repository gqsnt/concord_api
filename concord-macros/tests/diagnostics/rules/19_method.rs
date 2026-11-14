concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; METHOD "BAD/METHOD" fetch(); }
}
fn main() {}
