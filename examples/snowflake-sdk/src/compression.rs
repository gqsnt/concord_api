//! Provider-local gzip decoding, without changing reqwest's process-wide feature selection.
use std::io::Read;

use concord::{CodecError, Decode, Json, ResponseHead, bytes::Bytes};
use flate2::bufread::MultiGzDecoder;

use crate::{PartitionBodyError, model::PartitionBody};

pub(crate) struct PartitionBytes {
    bytes: Bytes,
    gzip: bool,
}

/// Its `Decode::ACCEPT` suggests `accept: application/json`.
///
/// Keep the declared operation's media check, identity and bounded wire read. Inflation needs
/// the client's runtime byte limit, so it follows the declared finite receive/decode step.
pub(crate) struct PartitionWire;
impl Decode<PartitionBytes> for PartitionWire {
    type Context = bool;
    const ACCEPT: Option<&'static str> = Some("application/json");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<PartitionBody>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<bool, CodecError> {
        let mut encodings = head.headers().get_all("content-encoding").iter();
        let Some(encoding) = encodings.next() else {
            return Ok(false);
        };
        if encodings.next().is_some() {
            return Err(CodecError::new("multiple partition content encodings"));
        }
        let encoding = encoding
            .to_str()
            .map_err(|_| CodecError::new("invalid partition content encoding"))?
            .trim();
        if encoding.eq_ignore_ascii_case("gzip") {
            Ok(true)
        } else if encoding.eq_ignore_ascii_case("identity") {
            Ok(false)
        } else {
            Err(CodecError::new("unsupported partition content encoding"))
        }
    }

    fn decode_body(gzip: bool, bytes: Bytes) -> Result<PartitionBytes, CodecError> {
        Ok(PartitionBytes { bytes, gzip })
    }
}

impl PartitionBytes {
    pub(crate) fn decode(self, limit: usize) -> Result<PartitionBody, PartitionBodyError> {
        if !self.gzip {
            return serde_json::from_slice(&self.bytes).map_err(PartitionBodyError::Json);
        }
        // The wire body was separately bounded by Concord. Read at most one byte beyond the
        // output cap; do not trust gzip ISIZE or Snowflake's descriptor as an allocation hint.
        // MultiGzDecoder validates every member/trailer and rejects junk after the gzip stream.
        let mut decoder =
            MultiGzDecoder::new(self.bytes.as_ref()).take((limit as u64).saturating_add(1));
        let mut decoded = Vec::new();
        decoder
            .read_to_end(&mut decoded)
            .map_err(PartitionBodyError::Gzip)?;
        if decoded.len() > limit {
            return Err(PartitionBodyError::Limit { limit });
        }
        serde_json::from_slice(&decoded).map_err(PartitionBodyError::Json)
    }
}
