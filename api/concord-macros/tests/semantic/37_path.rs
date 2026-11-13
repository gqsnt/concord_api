concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part fragment() { path "records"; } GET fetch(..fragment); }
}
fn main() {}
