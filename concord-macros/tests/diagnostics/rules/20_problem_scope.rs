concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope fragment { problem Issue; } }
}
fn main() {}
