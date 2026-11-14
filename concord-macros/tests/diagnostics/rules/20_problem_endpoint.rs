concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { problem Issue; } }
}
fn main() {}
