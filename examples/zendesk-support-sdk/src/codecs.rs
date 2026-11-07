//! Two formats of this slice: the create response with its idempotency lookup, and error bodies.

use concord::bytes::Bytes;
use concord::{CodecError, ContentTypeField, Decode, Json, ResponseHead, field};
use concord_codecs::{Charset, check_content_type};

use crate::{CreatedTicket, IdempotencyLookup, Problem, TicketResponse};

/// The response format of Create Ticket: strict [`Json`] for `{"ticket": ..}`, plus the
/// `x-idempotency-lookup` header read from the head before the body.
///
/// An unexpected lookup value never fails a create that succeeded: it becomes
/// [`IdempotencyLookup::Unrecognized`].
pub struct CreatedTicketFormat;

impl Decode<CreatedTicket> for CreatedTicketFormat {
    type Context = IdempotencyLookup;
    const ACCEPT: Option<&'static str> = Some("application/json");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<TicketResponse>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<IdempotencyLookup, CodecError> {
        let lookup = field::single(head.headers(), "x-idempotency-lookup");
        Ok(match lookup {
            Ok(None) => IdempotencyLookup::Absent,
            Ok(Some(one)) => match one.to_str().map(str::trim) {
                Ok(v) if v.eq_ignore_ascii_case("miss") => IdempotencyLookup::Miss,
                Ok(v) if v.eq_ignore_ascii_case("hit") => IdempotencyLookup::Hit,
                _ => IdempotencyLookup::Unrecognized,
            },
            Err(_) => IdempotencyLookup::Unrecognized,
        })
    }

    fn decode_body(
        idempotency: IdempotencyLookup,
        body: Bytes,
    ) -> Result<CreatedTicket, CodecError> {
        let response = <Json as Decode<TicketResponse>>::decode_body((), body)?;
        Ok(CreatedTicket {
            ticket: response.ticket,
            idempotency,
        })
    }
}

/// Error bodies: business-level JSON documents, `null`, or API-level `text/plain`.
pub(crate) struct ProblemFormat;

/// Which accepted media type the error body has.
pub(crate) enum ProblemMedia {
    Json,
    Text,
}

impl Decode<Problem> for ProblemFormat {
    type Context = ProblemMedia;

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_content_type(head, &["application/json", "text/plain"], Charset::Utf8)
    }

    fn context(head: &ResponseHead<'_>) -> Result<ProblemMedia, CodecError> {
        // `check_content_type` accepted exactly one of the two media types.
        let json = match head.content_type() {
            ContentTypeField::One(value) => value.to_str().is_ok_and(|text| {
                let essence = text.split(';').next().unwrap_or("").trim();
                essence.eq_ignore_ascii_case("application/json")
            }),
            ContentTypeField::Missing | ContentTypeField::Repeated => false,
        };
        Ok(if json {
            ProblemMedia::Json
        } else {
            ProblemMedia::Text
        })
    }

    fn decode_body(media: ProblemMedia, body: Bytes) -> Result<Problem, CodecError> {
        match media {
            ProblemMedia::Json => {
                let document: Option<crate::ErrorDocument> = serde_json::from_slice(&body)
                    .map_err(|e| {
                        CodecError::new("invalid Zendesk error document").with_source(e)
                    })?;
                Ok(document.map_or(Problem::Null, Problem::Document))
            }
            ProblemMedia::Text => String::from_utf8(body.into())
                .map(Problem::Text)
                .map_err(|e| CodecError::new("the error text is not UTF-8").with_source(e)),
        }
    }
}
