// The v2.1 dual metadata obligation is retired: `prepare` and the core
// `PreparedBody` constructors are the only source of the request media type,
// so a second `metadata` method is not part of the contract.
use concord_core::contracts::{BodyError, PreparedBody, RequestBodyContext, RequestRepresentation};

struct RetiredMetadataSpec;

impl RequestRepresentation<String> for RetiredMetadataSpec {
    fn metadata(_input: &String, _context: RequestBodyContext<'_>) -> Result<(), BodyError> {
        Ok(())
    }

    fn prepare(
        _input: String,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        Ok(PreparedBody::empty())
    }
}

fn main() {}
