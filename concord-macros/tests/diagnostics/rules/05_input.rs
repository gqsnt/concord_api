concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(Bad__Name: u8) { query { "wire" = Bad__Name; } } }
}
fn main() {}
