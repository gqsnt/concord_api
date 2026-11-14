concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part fragment() { idempotent; } scope branch(..fragment) { GET fetch(); } }
}
fn main() {}
