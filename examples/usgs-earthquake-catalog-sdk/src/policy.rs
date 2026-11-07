use crate::input::validate_magnitude_bounds;
use crate::{Error, HttpFailure, HttpKind, Magnitude, Timestamp, Window};
use concord::{Endpoint, SendWith};
use concord_codecs::Text;
use std::fmt;

/// Finite GeoJSON response limit (8 MiB). Streaming CSV has its own independent limits.
pub const SUCCESS_BODY_LIMIT: usize = 8 * 1024 * 1024;
/// Maximum retained error text (16 KiB).
pub const ERROR_BODY_LIMIT: usize = 16 * 1024;

/// A successful typed result together with its response head.
pub struct Response<T> {
    /// The GeoJSON value or unread incremental CSV reader.
    pub data: T,
    /// Status and headers; Debug withholds values.
    pub meta: concord::ResponseMeta,
}

impl<T> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

/// `send()` returns `Result<Response<T>, Error>`: one attempt, bounded bodies, preserved status.
#[derive(Default)]
pub struct Policy;

impl<T: Send> SendWith<T> for Policy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare()?;
        // Scope setters can replace time or magnitude bounds. Check their relations on the
        // prepared request so every generated send() validates the final values before I/O.
        let mut start = None;
        let mut end = None;
        let mut min_magnitude = None;
        let mut max_magnitude = None;
        for (key, value) in prepared.request().url().query_pairs() {
            match key.as_ref() {
                "starttime" => start = Some(Timestamp::parse(&value)?),
                "endtime" => end = Some(Timestamp::parse(&value)?),
                "minmagnitude" | "maxmagnitude" => {
                    let magnitude = Magnitude::new(
                        value
                            .parse()
                            .map_err(|_| Error::InvalidInput("magnitude must be a decimal"))?,
                    )?;
                    if key == "minmagnitude" {
                        min_magnitude = Some(magnitude);
                    } else {
                        max_magnitude = Some(magnitude);
                    }
                }
                _ => {}
            }
        }
        match (start, end) {
            (Some(start), Some(end)) => {
                Window::between(start, end)?;
            }
            (None, None) => {}
            _ => return Err(Error::InvalidInput("both window bounds are required")),
        }
        validate_magnitude_bounds(min_magnitude, max_magnitude)?;
        let received = prepared.receive().await?;
        let status = received.status().as_u16();
        if status != 200 {
            let kind = HttpKind::of(status);
            let (meta, document) = if status == 204 {
                (received.meta(), Ok(String::new()))
            } else {
                received.document::<Text, String>(ERROR_BODY_LIMIT).await
            };
            return Err(Error::Http(Box::new(HttpFailure {
                kind,
                meta,
                document,
            })));
        }
        let (data, meta) = received.decode_with_meta().await?;
        Ok(Response { data, meta })
    }
}
