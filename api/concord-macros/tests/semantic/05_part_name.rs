concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part Bad__Part() {} GET fetch(..Bad__Part); }
}
fn main() {}
