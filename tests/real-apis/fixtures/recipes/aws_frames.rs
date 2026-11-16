use concord::{bytes::Bytes, ByteStream};
use futures_util::TryStreamExt;
use std::io;
use tokio_util::{
    codec::{FramedRead, LengthDelimitedCodec},
    io::StreamReader,
};

concord::api! {
    pub client S3(origin: concord::Base, authorization: concord::Secret) {
        base origin;
        headers { "authorization" = authorization; }
        POST select(key: String, query: concord::Body) -> concord::ByteStream {
            path "{*key}";
            query { "select" = ""; "select-type" = 2; }
            body query;
            read concord::read::Stream;
        }
    }
}

/// Splits AWS event-stream messages, including the entire prelude and final CRC.
pub fn aws_frames(
    body: ByteStream,
) -> FramedRead<StreamReader<ByteStream, Bytes>, LengthDelimitedCodec> {
    let codec = LengthDelimitedCodec::builder()
        .length_field_type::<u32>()
        .length_adjustment(0)
        .num_skip(0)
        .max_frame_length(concord::MIB)
        .new_codec();
    FramedRead::new(StreamReader::new(body), codec)
}

/// Checks an S3 Select frame and returns its event type and payload.
pub fn select_event(frame: &[u8]) -> io::Result<(&[u8], &[u8])> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid AWS event frame.");
    if frame.len() < 16 {
        return Err(invalid());
    }
    let word = |bytes: &[u8]| -> io::Result<usize> {
        Ok(u32::from_be_bytes(bytes.try_into().map_err(|_| invalid())?) as usize)
    };
    let total = word(&frame[..4])?;
    let headers = word(&frame[4..8])?;
    if total != frame.len()
        || headers > total - 16
        || crc32fast::hash(&frame[..8]) as usize != word(&frame[8..12])?
        || crc32fast::hash(&frame[..total - 4]) as usize != word(&frame[total - 4..])?
    {
        return Err(invalid());
    }
    let mut fields = &frame[12..12 + headers];
    let mut message = None;
    let mut event = None;
    while !fields.is_empty() {
        let name_len = usize::from(fields[0]);
        let name = fields.get(1..1 + name_len).ok_or_else(invalid)?;
        let kind = *fields.get(1 + name_len).ok_or_else(invalid)?;
        fields = fields.get(2 + name_len..).ok_or_else(invalid)?;
        let size = match kind {
            0 | 1 => 0,
            2 => 1,
            3 => 2,
            4 => 4,
            5 | 8 => 8,
            9 => 16,
            6 | 7 => {
                let size = u16::from_be_bytes(
                    fields
                        .get(..2)
                        .ok_or_else(invalid)?
                        .try_into()
                        .map_err(|_| invalid())?,
                ) as usize;
                fields = &fields[2..];
                size
            }
            _ => return Err(invalid()),
        };
        let value = fields.get(..size).ok_or_else(invalid)?;
        fields = &fields[size..];
        if name == b":message-type" || name == b":event-type" {
            if kind != 7 {
                return Err(invalid());
            }
            let slot = if name == b":message-type" {
                &mut message
            } else {
                &mut event
            };
            if slot.replace(value).is_some() {
                return Err(invalid());
            }
        }
    }
    if message != Some(b"event".as_slice()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "S3 Select returned an error event.",
        ));
    }
    Ok((event.ok_or_else(invalid)?, &frame[12 + headers..total - 4]))
}

/// Keeps completed Records payloads and requires S3's terminal End event.
pub async fn read_select(body: ByteStream, records: &mut Vec<Bytes>) -> io::Result<()> {
    let mut frames = aws_frames(body);
    while let Some(frame) = frames.try_next().await? {
        let (event, payload) = select_event(&frame)?;
        match event {
            b"Records" => records.push(Bytes::copy_from_slice(payload)),
            b"End" => return Ok(()),
            b"Stats" | b"Progress" | b"Cont" => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Unknown S3 Select event.",
                ))
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "S3 Select ended without an End event.",
    ))
}
