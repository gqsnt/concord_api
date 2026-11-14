concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(spare: u16); }
}
fn main() {}
