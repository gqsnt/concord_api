concord::api! {
    client Workshop { base "https://part.test"; read Reader; scope branch { status { 202 { body } => unpack(body) } GET fetch() -> Option<Record>; } }
}
fn main() {}
