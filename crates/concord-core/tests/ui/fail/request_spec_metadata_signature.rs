use concord_core::contracts::{BodyError, RequestBody, RequestBodyContext, RequestRepresentation};

struct MalformedMetadataSpec;

impl RequestRepresentation<String> for MalformedMetadataSpec {
    fn metadata(
        _input: &String,
        _context: RequestBodyContext<'_>,
    ) -> Result<String, BodyError> {
        Ok(String::new())
    }

    fn prepare(
        _input: String,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::empty())
    }
}

fn main() {}
