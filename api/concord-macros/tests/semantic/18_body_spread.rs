concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part fragment() { body make(); } scope branch(..fragment) { GET fetch(); } }
}
fn main() {}
