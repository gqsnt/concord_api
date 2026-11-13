concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { #[doc="why"] path "records"; } }
}
fn main() {}
