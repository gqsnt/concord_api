concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part fragment() { problem Issue; } GET fetch(..fragment); }
}
fn main() {}
