// `send value;` names a required client value; an optional or defaulted one is refused.
#[derive(Clone)]
pub struct Signer;
concord::api! {
    pub client Optional(signer?: Signer) {
        base "http://x.test";
        send signer;
        GET one();
    }
}
concord::api! {
    pub client Defaulted(signer: Signer = Signer) {
        base "http://x.test";
        send signer;
        GET one();
    }
}
fn main() {}
