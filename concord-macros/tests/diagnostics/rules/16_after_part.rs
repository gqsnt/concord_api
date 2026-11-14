concord::api! {
    client Workshop { part fragment() {} read Reader; GET fetch(..fragment); }
}
fn main() {}
