concord::api! {
    client Workshop { base "https://part.test"; read Reader; part answer() { status { 202 { body } => unpack(body) } } GET fetch(..answer); }
}
fn main() {}
