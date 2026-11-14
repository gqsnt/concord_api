concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { path "records?sort=new"; } }
}
fn main() {}
