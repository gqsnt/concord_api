use concord::advanced::{CodecError, CredentialMaterial, DecodeBody, DecodeContext};
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
        base "http://storage.example.test/";
        credentials {
            credential session = endpoint Source;
        }
    }

    GET Source() -> crate::SessionCodec;
    GET Other() -> Text;
}

fn main() {
    let client = StorageApi::new().unwrap();
    let _ = client.other().store_as_session();
}
