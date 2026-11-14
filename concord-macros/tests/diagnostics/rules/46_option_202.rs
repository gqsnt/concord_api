concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; status { 202 { body } => unpack(body) } GET fetch() -> Option<Record>; }
}
fn main() {}
