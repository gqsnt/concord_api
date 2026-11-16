//! A body-held permit preserves frames and all response metadata.
use super::engine::Permit;
use concord_core::http;
use concord_core::reqwest::{self, Response, ResponseBuilderExt};
use http_body::{Body, Frame, SizeHint};
use std::pin::Pin;
use std::task::{Context, Poll};

struct Held {
    body: reqwest::Body,
    permit: Option<Permit>,
}

impl Body for Held {
    type Data = concord_core::bytes::Bytes;
    type Error = <reqwest::Body as Body>::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let frame = Pin::new(&mut self.body).poll_frame(cx);
        if matches!(frame, Poll::Ready(None | Some(Err(_)))) || self.body.is_end_stream() {
            self.permit.take();
        }
        frame
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

pub(super) fn hold(response: Response, permit: Permit) -> Response {
    let url = response.url().clone();
    let mut raw: http::Response<reqwest::Body> = response.into();
    // Only an extension is set, so this builder has no fallible inputs.
    let holder = http::Response::builder()
        .url(url)
        .body(())
        .unwrap_or_else(|_| http::Response::new(()));
    raw.extensions_mut()
        .extend(holder.into_parts().0.extensions);
    raw.map(|body| {
        let permit = if body.is_end_stream() {
            None
        } else {
            Some(permit)
        };
        reqwest::Body::wrap(Held { body, permit })
    })
    .into()
}
