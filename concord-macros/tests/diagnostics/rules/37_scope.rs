concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part fragment() { scope nested {} } GET fetch(..fragment); }
}
fn main() {}
