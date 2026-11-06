//! Four declared operations; HTTP execution remains in Concord.
use crate::compression::{PartitionBytes, PartitionWire};
use crate::{RequestId, ResultSet, ServerMessage, Statement, StatementHandle};
use concord::reqwest::header::{AUTHORIZATION, HeaderValue, USER_AGENT};
use concord::{CodecError, Decode, Json, ResponseHead};

pub(crate) enum StatementBody {
    Pending(ServerMessage),
    Complete(ResultSet),
}
pub(crate) struct StatementJson;
impl Decode<StatementBody> for StatementJson {
    type Context = u16;
    const ACCEPT: Option<&'static str> = Some("application/json");
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<ServerMessage>>::check_content_type(head)
    }
    fn context(head: &ResponseHead<'_>) -> Result<u16, CodecError> {
        Ok(head.status().as_u16())
    }
    fn decode_body(status: u16, body: concord::bytes::Bytes) -> Result<StatementBody, CodecError> {
        match status {
            202 => {
                <Json as Decode<ServerMessage>>::decode_body((), body).map(StatementBody::Pending)
            }
            200 => <Json as Decode<ResultSet>>::decode_body((), body).map(StatementBody::Complete),
            _ => Err(CodecError::new("unexpected statement response status")),
        }
    }
}

concord::api! {
    pub(crate) client SqlApi(
        #[sensitive] header(AUTHORIZATION) authorization: HeaderValue,
        header("X-Snowflake-Authorization-Token-Type") token_type: String,
        header(USER_AGENT) user_agent: HeaderValue,
    ) {
        base "https://account.snowflake.invalid";
        path "api/v2/statements";
        headers { "Content-Type" = "application/json"; }
        format Json;

        POST submit(query("requestId") request_id: RequestId, body statement: Statement) -> StatementBody {
            query { "async" = "true"; }
            response StatementJson;
        }
        scope statement(handle: StatementHandle) {
            path "{handle}";
            GET status() -> StatementBody { response StatementJson; }
            GET partition(query partition: usize) -> PartitionBytes {
                headers { "Accept-Encoding" = "gzip"; }
                response PartitionWire;
            }
            POST cancel(query("requestId") request_id: RequestId) -> ServerMessage { path "cancel"; }
        }
    }
}
