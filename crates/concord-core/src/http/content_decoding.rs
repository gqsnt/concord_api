//! Recognition of supported content codings at the private native boundary.
//!
//! Reqwest's pinned decoder recognizes only lower-case tokens. RFC 9110
//! section 8.4.1 makes their case insignificant. The fallback below uses the
//! same lazy decoder, only for a supported enabled token left undecoded by
//! Reqwest. Acquisition, deadlines, byte meters and leases keep their owners.

use super::config::HttpConfig;

#[derive(Clone, Copy, Default)]
pub(super) struct ContentDecoding {
    #[cfg(feature = "gzip")]
    gzip: bool,
    #[cfg(feature = "brotli")]
    brotli: bool,
    #[cfg(feature = "deflate")]
    deflate: bool,
}

impl ContentDecoding {
    pub(super) const fn disabled() -> Self {
        Self {
            #[cfg(feature = "gzip")]
            gzip: false,
            #[cfg(feature = "brotli")]
            brotli: false,
            #[cfg(feature = "deflate")]
            deflate: false,
        }
    }

    pub(super) const fn from_config(config: &HttpConfig) -> Self {
        let _ = config;
        Self {
            #[cfg(feature = "gzip")]
            gzip: !config.disable_gzip,
            #[cfg(feature = "brotli")]
            brotli: !config.disable_brotli,
            #[cfg(feature = "deflate")]
            deflate: !config.disable_deflate,
        }
    }

    pub(super) async fn response(self, response: reqwest::Response) -> reqwest::Response {
        #[cfg(any(feature = "gzip", feature = "brotli", feature = "deflate"))]
        {
            use http::header::CONTENT_ENCODING;
            use reqwest::ResponseBuilderExt as _;
            use tower_service::Service as _;

            // A list or stacked coding is not a single recognized token.
            // Do not broaden the pinned native decoder's grammar here.
            if response.headers().get_all(CONTENT_ENCODING).iter().count() != 1 {
                return response;
            }
            let Some(value) = response.headers().get(CONTENT_ENCODING) else {
                return response;
            };
            let value = value.as_bytes();
            let canonical = self.canonical(value);
            let Some(canonical) = canonical.filter(|coding| value != coding.as_bytes()) else {
                return response;
            };
            let url = response.url().clone();
            let mut native: http::Response<reqwest::Body> = response.into();
            // Reqwest's Response -> http::Response conversion drops its URL.
            // Its public builder extension restores that fact without changing
            // any other head, status, version or extension.
            let url_parts = http::Response::builder()
                .url(url)
                .body(())
                .expect("a URL extension cannot invalidate a response builder")
                .into_parts()
                .0;
            native.extensions_mut().extend(url_parts.extensions);
            native
                .headers_mut()
                .insert(CONTENT_ENCODING, http::HeaderValue::from_static(canonical));
            let mut decoder =
                tower_http::decompression::Decompression::new(Received(Some(native))).no_zstd();
            #[cfg(feature = "gzip")]
            {
                decoder = decoder.gzip(self.gzip);
            }
            #[cfg(feature = "brotli")]
            {
                decoder = decoder.br(self.brotli);
            }
            #[cfg(feature = "deflate")]
            {
                decoder = decoder.deflate(self.deflate);
            }
            // Received resolves immediately and never polls the body or sends
            // a request. The body remains lazy under the existing consumers.
            let native = match decoder.call(http::Request::new(())).await {
                Ok(native) => native,
                Err(never) => match never {},
            };
            let (parts, body) = native.into_parts();
            reqwest::Response::from(http::Response::from_parts(parts, reqwest::Body::wrap(body)))
        }
        #[cfg(not(any(feature = "gzip", feature = "brotli", feature = "deflate")))]
        {
            let _ = self;
            response
        }
    }

    #[cfg(any(feature = "gzip", feature = "brotli", feature = "deflate"))]
    fn canonical(self, value: &[u8]) -> Option<&'static str> {
        #[cfg(feature = "gzip")]
        if self.gzip && value.eq_ignore_ascii_case(b"gzip") {
            return Some("gzip");
        }
        #[cfg(feature = "brotli")]
        if self.brotli && value.eq_ignore_ascii_case(b"br") {
            return Some("br");
        }
        #[cfg(feature = "deflate")]
        if self.deflate && value.eq_ignore_ascii_case(b"deflate") {
            return Some("deflate");
        }
        None
    }
}

#[cfg(any(feature = "gzip", feature = "brotli", feature = "deflate"))]
struct Received(Option<http::Response<reqwest::Body>>);

#[cfg(any(feature = "gzip", feature = "brotli", feature = "deflate"))]
impl tower_service::Service<http::Request<()>> for Received {
    type Response = http::Response<reqwest::Body>;
    type Error = std::convert::Infallible;
    type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: http::Request<()>) -> Self::Future {
        std::future::ready(Ok(self
            .0
            .take()
            .expect("one received response is decoded once")))
    }
}

#[cfg(all(test, any(feature = "gzip", feature = "brotli", feature = "deflate")))]
mod s3_boundary_tests {
    use super::ContentDecoding;
    use bytes::Bytes;
    use http::header::{CONTENT_ENCODING, CONTENT_LENGTH};
    use reqwest::ResponseBuilderExt as _;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    #[derive(Clone, Debug, PartialEq)]
    struct Marker(u32);

    struct CountedBody {
        polls: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
    }

    impl http_body::Body for CountedBody {
        type Data = Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Pending
        }
        fn size_hint(&self) -> http_body::SizeHint {
            http_body::SizeHint::with_exact(17)
        }
    }

    impl Drop for CountedBody {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn enabled() -> ContentDecoding {
        ContentDecoding {
            #[cfg(feature = "gzip")]
            gzip: true,
            #[cfg(feature = "brotli")]
            brotli: true,
            #[cfg(feature = "deflate")]
            deflate: true,
        }
    }

    fn counted_response(
        codings: &[&str],
    ) -> (reqwest::Response, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let polls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let body = reqwest::Body::wrap(CountedBody {
            polls: Arc::clone(&polls),
            drops: Arc::clone(&drops),
        });
        let mut native = http::Response::builder()
            .status(http::StatusCode::ACCEPTED)
            .version(http::Version::HTTP_2)
            .header(CONTENT_LENGTH, "17")
            .header("x-owned-s3-head", "preserved")
            .extension(Marker(0x5333))
            .url(url::Url::parse("https://owned.example/s3?q=1").expect("fixture URL"))
            .body(body)
            .expect("fixture response");
        for coding in codings {
            native.headers_mut().append(
                CONTENT_ENCODING,
                http::HeaderValue::from_str(coding).expect("fixture coding"),
            );
        }
        (reqwest::Response::from(native), polls, drops)
    }

    fn assert_head(response: &reqwest::Response) {
        assert_eq!(response.url().as_str(), "https://owned.example/s3?q=1");
        assert_eq!(response.status(), http::StatusCode::ACCEPTED);
        assert_eq!(response.version(), http::Version::HTTP_2);
        assert_eq!(response.headers()["x-owned-s3-head"], "preserved");
        assert_eq!(response.extensions().get::<Marker>(), Some(&Marker(0x5333)));
    }

    #[tokio::test]
    async fn s3_boundary_enabled_codings_wrap_without_poll_and_keep_head() {
        let codings: &[&str] = &[
            #[cfg(feature = "gzip")]
            "GZip",
            #[cfg(feature = "brotli")]
            "BR",
            #[cfg(feature = "deflate")]
            "Deflate",
        ];
        for coding in codings {
            let (response, polls, drops) = counted_response(&[coding]);
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                enabled().response(response),
            )
            .await
            .expect("normalization must not wait for the body");
            assert_head(&response);
            assert!(!response.headers().contains_key(CONTENT_ENCODING));
            assert!(!response.headers().contains_key(CONTENT_LENGTH));
            assert_eq!(response.content_length(), None);
            assert_eq!(polls.load(Ordering::SeqCst), 0);
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            drop(response);
            assert_eq!(polls.load(Ordering::SeqCst), 0);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn s3_boundary_disabled_unknown_stacked_multiple_and_identity_are_untouched() {
        let cases: &[(&[&str], bool)] = &[
            (&[], false),
            (&["GZip"], true),
            (&["BR"], true),
            (&["Deflate"], true),
            (&["owned-unknown"], false),
            (&["gzip, br"], false),
            (&["GZip", "br"], false),
            (&["gzip"], false),
        ];
        for (codings, disabled) in cases {
            let (response, polls, drops) = counted_response(codings);
            let original_headers = response.headers().clone();
            let decoding = if *disabled {
                ContentDecoding::disabled()
            } else {
                enabled()
            };
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                decoding.response(response),
            )
            .await
            .expect("identity normalization must not wait for the body");
            assert_head(&response);
            assert_eq!(response.headers(), &original_headers);
            assert_eq!(response.content_length(), Some(17));
            assert_eq!(polls.load(Ordering::SeqCst), 0);
            drop(response);
            assert_eq!(polls.load(Ordering::SeqCst), 0);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn s3_boundary_each_disabled_algorithm_is_independent() {
        #[cfg(feature = "gzip")]
        {
            let mut decoding = enabled();
            decoding.gzip = false;
            assert_eq!(decoding.canonical(b"GZip"), None);
            #[cfg(feature = "brotli")]
            assert_eq!(decoding.canonical(b"BR"), Some("br"));
            #[cfg(feature = "deflate")]
            assert_eq!(decoding.canonical(b"Deflate"), Some("deflate"));
        }
        #[cfg(feature = "brotli")]
        {
            let mut decoding = enabled();
            decoding.brotli = false;
            assert_eq!(decoding.canonical(b"BR"), None);
            #[cfg(feature = "gzip")]
            assert_eq!(decoding.canonical(b"GZip"), Some("gzip"));
            #[cfg(feature = "deflate")]
            assert_eq!(decoding.canonical(b"Deflate"), Some("deflate"));
        }
        #[cfg(feature = "deflate")]
        {
            let mut decoding = enabled();
            decoding.deflate = false;
            assert_eq!(decoding.canonical(b"Deflate"), None);
            #[cfg(feature = "gzip")]
            assert_eq!(decoding.canonical(b"GZip"), Some("gzip"));
            #[cfg(feature = "brotli")]
            assert_eq!(decoding.canonical(b"BR"), Some("br"));
        }
    }
}
