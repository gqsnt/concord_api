concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; HEAD fetch() -> u8; }
}
fn main() {}
