concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope fragment { part nested() {} } }
}
fn main() {}
