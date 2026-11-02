// Format row 9: where a blanket impl's bound fails (serde's `DeserializeOwned` for `json`), rustc
// reports that bound instead of Concord's message, still at the payload (inherited default) or at
// the word (in place).
concord::api! {
    pub client Api {
        base "http://x.test";
        response json;
        GET image() -> concord::bytes::Bytes;
        GET thumbnail() -> json concord::bytes::Bytes;
    }
}
fn main() {}
