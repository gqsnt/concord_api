// Format row 9: where a blanket impl's bound fails (serde's `DeserializeOwned` for `json`), rustc
// reports that bound instead of Concord's message, still at the payload (inherited default) or at
// the statement's format (on the endpoint).
use concord::Json;
concord::api! {
    pub client Api {
        base "http://x.test";
        response Json;
        GET image() -> concord::bytes::Bytes;
        GET thumbnail() -> concord::bytes::Bytes { response Json; }
    }
}
fn main() {}
