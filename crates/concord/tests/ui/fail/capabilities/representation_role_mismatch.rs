// DIAG-01 (§19.1) / §9.1: representation roles are selected by direction.
// A request-only representation selected for a result, or a response-only
// representation selected for an (optional) body, reports the stable
// `[C3-REP]` role message with a correction at the selecting type: the
// inline `via` type, or the owner's `response R;` default it inherits.
use concord::body::{BodyError, PreparedBody, RequestBodyContext, RequestRepresentation};
use concord::response::{DiscardDriver, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord::{Binary, Bytes, api};

pub struct ResponseOnly;

impl ResponseRepresentation<()> for ResponseOnly {
    type Driver = DiscardDriver;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

pub struct RequestOnly;

impl RequestRepresentation<String> for RequestOnly {
    fn prepare(input: String, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        PreparedBody::bytes(Bytes::from(input)).content_type("text/plain")
    }
}

api! {
    client InlineResponseRoleApi {
        base "https://roles.example.test";

        GET read() -> String via crate::RequestOnly;
    }
}

api! {
    client InheritedResponseRoleApi {
        base "https://roles.example.test";
        response crate::RequestOnly;

        GET read() -> String;
    }
}

api! {
    client OptionalBodyRoleApi {
        base "https://roles.example.test";

        POST write(body input?: Bytes via crate::ResponseOnly) -> Bytes via Binary;
    }
}

fn main() {}
