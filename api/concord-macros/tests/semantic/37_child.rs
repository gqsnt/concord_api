concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part fragment() { GET nested(); } GET fetch(..fragment); }
}
fn main() {}
