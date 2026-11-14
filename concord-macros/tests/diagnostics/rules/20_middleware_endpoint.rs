concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { middleware Layer; } }
}
fn main() {}
