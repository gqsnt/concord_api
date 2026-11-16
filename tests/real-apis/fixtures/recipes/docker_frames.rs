use concord::{bytes::Bytes, ByteStream};
use futures_util::TryStreamExt;
use std::io;
use tokio_util::{
    codec::{FramedRead, LengthDelimitedCodec},
    io::StreamReader,
};

concord::api! {
    pub client Docker(origin: concord::Base) {
        base origin;
        GET logs(container: String) -> concord::ByteStream {
            path "containers/{container}/logs";
            query { "stdout" = true; "stderr" = true; "follow" = true; }
            read concord::read::Stream;
        }
    }
}

/// Splits non-TTY Docker logs, allowing at most 64 KiB of payload per frame.
pub fn docker_frames(
    body: ByteStream,
) -> FramedRead<StreamReader<ByteStream, Bytes>, LengthDelimitedCodec> {
    let codec = LengthDelimitedCodec::builder()
        .length_field_offset(4)
        .length_field_type::<u32>()
        .length_adjustment(8)
        .num_skip(0)
        .max_frame_length(64 * concord::KIB)
        .new_codec();
    FramedRead::new(StreamReader::new(body), codec)
}

/// Validates the complete prefix and returns its channel and uninterpreted payload.
pub fn docker_log(frame: Bytes) -> io::Result<(u8, Bytes)> {
    if frame.len() < 8 || !matches!(frame[0], 0..=2) || frame[1..4] != [0, 0, 0] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid Docker log prefix.",
        ));
    }
    Ok((frame[0], frame.slice(8..)))
}

/// Keeps completed log records until EOF, leaving them available on a later failure.
pub async fn read_logs(body: ByteStream, logs: &mut Vec<(u8, Bytes)>) -> io::Result<()> {
    let mut frames = docker_frames(body);
    while let Some(frame) = frames.try_next().await? {
        logs.push(docker_log(frame.freeze())?);
    }
    Ok(())
}
