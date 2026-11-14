concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope fragment { middleware Layer; } }
}
fn main() {}
