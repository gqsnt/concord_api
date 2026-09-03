use concord::auth::CredentialMaterial;
use concord::codec::{CodecError, DecodeBody, DecodeContext};
use concord::{Bytes, Text, api};

#[derive(Clone)]
pub struct SessionToken;

impl CredentialMaterial for SessionToken {}

pub struct SessionCodec;

impl DecodeBody for SessionCodec {
    type Output = SessionToken;

    const ACCEPT: Option<&'static str> = None;

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(SessionToken)
    }
}

api! {
    client StorageApi {
        timeout off;
        base "http://storage.example.test/";
        credentials {
            credential session = endpoint source;
        }


    GET source() -> crate::SessionToken using crate::SessionCodec;
    GET other() -> String using Text;
}}

fn main() {
    let client = StorageApi::new().unwrap();
    let _ = client.other().store_as_session();
}
