use concord_core::contracts::{BodyError, RequestBody, RequestBodyContext, RequestBodySpec};

struct MalformedMetadataSpec;

impl RequestBodySpec for MalformedMetadataSpec {
    type Input = String;

    fn metadata(
        _input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<String, BodyError> {
        Ok(String::new())
    }

    fn prepare(
        _input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::empty())
    }
}

fn main() {}
