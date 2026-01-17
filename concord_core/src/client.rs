use crate::codec::{ContentType, Decodes, Encodes};
use crate::debug::DebugLevel;
use crate::endpoint::{BodyPart, Endpoint, PolicyPart, ResponseSpec, RoutePart};
use crate::error::ApiClientError;
use crate::pagination::Caps;
use crate::policy::{Policy, PolicyLayer, PolicyPatch};
use crate::request::PendingRequest;
use crate::transport::{BuiltRequest, BuiltResponse, DecodedResponse, RequestMeta};
use crate::transport::{ReqwestTransport, Transport, TransportBody, TransportError};
use crate::types::RouteParts;
use bytes::Bytes;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use http::uri::Scheme;

pub trait ClientContext: Sized {
    type Vars: Clone + Send + Sync + 'static;
    const SCHEME: Scheme;
    const DOMAIN: &'static str;

    fn base_route(_vars: &Self::Vars) -> RouteParts {
        RouteParts::new()
    }

    fn base_policy(_vars: &Self::Vars) -> Result<Policy, ApiClientError> {
        Ok(Policy::new())
    }
}

#[derive(Clone)]
pub struct ApiClient<Cx: ClientContext, T: Transport = ReqwestTransport> {
    transport: T,
    vars: Cx::Vars,
    debug_level: DebugLevel,
    pagination_caps: Caps,
}

impl<Cx: ClientContext> ApiClient<Cx, ReqwestTransport> {
    pub fn new(vars: Cx::Vars) -> Self {
        Self::with_reqwest_client(vars, reqwest::Client::new())
    }

    pub fn with_reqwest_client(vars: Cx::Vars, client: reqwest::Client) -> Self {
        Self::with_transport(vars, ReqwestTransport::new(client))
    }
}
impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub fn with_transport(vars: Cx::Vars, transport: T) -> Self {
        Self {
            transport,
            vars,
            debug_level: DebugLevel::default(),
            pagination_caps: Caps::default(),
        }
    }

    #[inline]
    pub fn vars(&self) -> &Cx::Vars {
        &self.vars
    }

    #[inline]
    pub fn vars_mut(&mut self) -> &mut Cx::Vars {
        &mut self.vars
    }

    #[inline]
    pub fn set_vars(&mut self, vars: Cx::Vars) {
        self.vars = vars;
    }

    #[inline]
    pub fn update_vars(&mut self, f: impl FnOnce(&mut Cx::Vars)) {
        f(&mut self.vars);
    }
    #[inline]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    #[inline]
    pub fn debug_level(&self) -> DebugLevel {
        self.debug_level
    }

    #[inline]
    pub fn set_debug_level(&mut self, level: DebugLevel) {
        self.debug_level = level;
    }



    #[inline]
    pub fn pagination_caps(&self) -> Caps {
        self.pagination_caps
    }

    #[inline]
    pub fn set_pagination_caps(&mut self, caps: Caps) {
        self.pagination_caps = caps;
    }

    #[inline]
    pub fn with_pagination_caps(mut self, caps: Caps) -> Self {
        self.pagination_caps = caps;
        self
    }

    #[inline]
    pub fn with_debug_level(mut self, level: DebugLevel) -> Self {
        self.debug_level = level;
        self
    }

    #[inline]
    pub fn request<E>(&self, ep: E) -> PendingRequest<'_, Cx, E, T>
    where
        E: Endpoint<Cx>,
    {
        PendingRequest::new(self, ep)
    }

    pub(crate) async fn execute_decoded_ref_with<E, F>(
        &self,
        ep: &E,
        meta: RequestMeta,
        dbg: DebugLevel,
        patch_policy: F,
    ) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError>
    where
        E: Endpoint<Cx>,
        F: for<'a> FnOnce(&mut PolicyPatch<'a>) -> Result<(), ApiClientError>,
    {
        let dbg_verbose = dbg.is_verbose();
        let dbg_vv = dbg.is_very_verbose();

        let built = self
            .build_request::<E, F>(ep, meta, patch_policy)
            .map_err(|e| ApiClientError::in_endpoint(ep.name(), e))?;
        let url_str = built.url.as_str().to_string();

        if dbg_verbose {
            if built.meta.page_index == 0 {
                eprintln!(
                    "[client_api:{}] -> {} {} ({})",
                    dbg,
                    E::METHOD,
                    url_str,
                    ep.name()
                );
            } else {
                eprintln!(
                    "[client_api:{}] -> {} {} ({}) page={}",
                    dbg,
                    E::METHOD,
                    url_str,
                    ep.name(),
                    built.meta.page_index
                );
            }
        }

        if dbg_vv {
            eprintln!("[client_api:{}] request headers:", dbg);
            for (k, v) in built.headers.iter() {
                let vs = v.to_str().unwrap_or("<non-utf8>");
                eprintln!("  {}: {}", k, vs);
            }
            if let Some(body) = built.body.as_ref() {
                const MAX_CHARS: usize = 32 * 1024;
                let s = format_request_body_for_debug::<Cx, E>(body, MAX_CHARS);
                eprintln!(
                    "[client_api:{}] request body ({} bytes): {}",
                    dbg,
                    body.len(),
                    s
                );
            }
        }
        // 2) Send
        let resp = self
            .send_built_request(built, dbg, dbg_verbose, dbg_vv, &url_str)
            .await
            .map_err(|e| ApiClientError::in_endpoint(ep.name(), e))?;
        if dbg_verbose {
            eprintln!(
                "[client_api:{}] <- {} {} (ok)",
                dbg,
                resp.status.as_u16(),
                url_str
            );
        }
        if dbg_vv {
            const MAX_CHARS: usize = 32 * 1024;
            let s = format_response_body_for_debug::<Cx, E>(&resp.body, MAX_CHARS);
            eprintln!("[client_api:{}] response headers:", dbg);
            for (k, v) in resp.headers.iter() {
                let vs = v.to_str().unwrap_or("<non-utf8>");
                eprintln!("  {}: {}", k, vs);
            }
            eprintln!(
                "[client_api:{}] response body ({} bytes): {}",
                dbg,
                resp.body.len(),
                s
            );
        }

        Self::decode_built_response::<E>(resp)
            .map_err(|e| ApiClientError::in_endpoint(ep.name(), e))
    }
}

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn build_request<E, F>(
        &self,
        ep: &E,
        meta: RequestMeta,
        patch_policy: F,
    ) -> Result<BuiltRequest, ApiClientError>
    where
        E: Endpoint<Cx>,
        F: for<'a> FnOnce(&mut PolicyPatch<'a>) -> Result<(), ApiClientError>,
    {
        // Route = base + endpoint route part
        let mut route = Cx::base_route(self.vars());
        <E::Route as RoutePart<Cx, E>>::apply(ep, self.vars(), &mut route)?;

        // Policy layering model:
        // client (base_policy) -> (prefix/path) -> endpoint -> runtime injections
        let mut policy = Cx::base_policy(self.vars())?;
        policy.set_layer(PolicyLayer::Endpoint);
        <E::Policy as PolicyPart<Cx, E>>::apply(ep, self.vars(), &mut policy)?;

        // Runtime Accept injection (decoder-owned) after endpoint policy.
        policy.set_layer(PolicyLayer::Runtime);
        let is_head = E::METHOD == http::Method::HEAD;
        if !is_head && !E::response_is_no_content() {
            policy.ensure_accept(E::accept_content_type());
        }

        // Runtime patch (pagination controller, etc.)
        {
            let mut patch = PolicyPatch::new(&mut policy);
            patch_policy(&mut patch)?;
        }

        // Compute parts after patch (Content-Type may have been added/removed there).
        let (mut headers, query, timeout) = policy.into_parts();

        // URL
        route
            .host()
            .validate(ep.name())
            .map_err(|e| ApiClientError::in_endpoint(ep.name(), e))?;
        let host = route.host().join(Cx::DOMAIN);
        let base = format!("{}://{}", Cx::SCHEME, host);
        let mut url = url::Url::parse(&base)?;
        url.set_path(route.path().as_str());
        {
            let mut qp = url.query_pairs_mut();
            for (k, v) in query.iter() {
                qp.append_pair(k, v);
            }
        }

        // Body (optional) + Content-Type injection if missing.
        let mut body_bytes: Option<Bytes> = None;
        if let Some(body) = <E::Body as BodyPart<E>>::body(ep) {
            let encoded = <<E::Body as BodyPart<E>>::Enc as Encodes<
                <E::Body as BodyPart<E>>::Body,
            >>::encode(body)
            .map_err(ApiClientError::codec_error)?;

            if !headers.contains_key(CONTENT_TYPE) {
                let ct = <<E::Body as BodyPart<E>>::Enc as ContentType>::CONTENT_TYPE;
                if !ct.is_empty() {
                    headers.insert(CONTENT_TYPE, http::HeaderValue::from_static(ct));
                }
            }
            body_bytes = Some(encoded);
        }

        Ok(BuiltRequest {
            meta,
            url,
            headers,
            body: body_bytes,
            timeout,
        })
    }

    async fn send_built_request(
        &self,
        built: BuiltRequest,
        dbg: DebugLevel,
        dbg_verbose: bool,
        dbg_vv: bool,
        url_str: &str,
    ) -> Result<BuiltResponse, ApiClientError> {
        let mut resp = self.transport.send(built).await?;
        let status = resp.status;
        let resp_headers = resp.headers;

        if !status.is_success() {
            let full_len = resp.content_length.map(|n| n as usize);
            let preview_bytes = read_body_preview(resp.body.as_mut(), 8 * 1024).await?;
            let preview = crate::error::body_as_text(&resp_headers, &preview_bytes, full_len);
            if dbg_verbose {
                eprintln!(
                    "[client_api:{}] <- {} {} (error)",
                    dbg,
                    status.as_u16(),
                    url_str
                );
            }
            if dbg_vv {
                eprintln!("[client_api:{}] response headers:", dbg);
                for (k, v) in resp_headers.iter() {
                    let vs = v.to_str().unwrap_or("<non-utf8>");
                    eprintln!("  {}: {}", k, vs);
                }
                eprintln!("[client_api:{}] response body preview: {}", dbg, preview);
            }
            return Err(ApiClientError::HttpStatus {
                status,
                headers: resp_headers,
                body: preview,
            });
        }

        let bytes = read_body_all(resp.body.as_mut()).await?;
        Ok(BuiltResponse {
            meta: resp.meta,
            url: resp.url,
            status,
            headers: resp_headers,
            body: bytes,
        })
    }

    fn decode_built_response<E>(
        resp: BuiltResponse,
    ) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError>
    where
        E: Endpoint<Cx>,
    {
        // Enforce the documented constraints:
        // - HEAD must map to a NoContent decoder (body is empty by definition).
        if resp.meta.method == http::Method::HEAD && !E::response_is_no_content() {
            return Err(ApiClientError::HeadRequiresNoContent {
                endpoint: resp.meta.endpoint,
            });
        }

        // - 204/205 are "no content" success statuses. If the endpoint expects content, fail early with a clear error.
        if matches!(
            resp.status,
            StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT
        ) && !E::response_is_no_content()
        {
            return Err(ApiClientError::NoContentStatusRequiresNoContent {
                endpoint: resp.meta.endpoint,
                status: resp.status,
            });
        }

        let decoded = <<E::Response as ResponseSpec>::Dec as Decodes<
            <E::Response as ResponseSpec>::Decoded,
        >>::decode(&resp.body)
        .map_err(|e| ApiClientError::Decode {
            source: e.into(),
            body: crate::error::body_as_text(&resp.headers, &resp.body, Some(resp.body.len())),
        })?;

        let endpoint = resp.meta.endpoint;
        let decoded_resp = DecodedResponse {
            meta: resp.meta,
            url: resp.url,
            status: resp.status,
            headers: resp.headers,
            value: decoded,
        };
        <E::Response as ResponseSpec>::map_response(decoded_resp).map_err(|e| ApiClientError::Transform {
            endpoint,
            source: e,
        })
    }
}

fn format_request_body_for_debug<Cx, E>(bytes: &Bytes, max_chars: usize) -> String
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    crate::codec::format_debug_body::<<E::Body as BodyPart<E>>::Enc>(bytes, max_chars)
}

fn format_response_body_for_debug<Cx, E>(bytes: &Bytes, max_chars: usize) -> String
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    crate::codec::format_debug_body::<<E::Response as ResponseSpec>::Dec>(bytes, max_chars)
}

async fn read_body_preview(
    body: &mut dyn TransportBody,
    max: usize,
) -> Result<Bytes, TransportError> {
    let mut buf = bytes::BytesMut::with_capacity(max.min(8 * 1024));
    while buf.len() < max {
        match body.next_chunk().await? {
            Some(chunk) => {
                let remaining = max - buf.len();
                if chunk.len() <= remaining {
                    buf.extend_from_slice(&chunk);
                } else {
                    buf.extend_from_slice(&chunk[..remaining]);
                    break;
                }
            }
            None => break,
        }
    }
    Ok(buf.freeze())
}

async fn read_body_all(body: &mut dyn TransportBody) -> Result<Bytes, TransportError> {
    let mut buf = bytes::BytesMut::with_capacity(8 * 1024);
    while let Some(chunk) = body.next_chunk().await? {
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.freeze())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::codec::{
        ContentType, Encodes, Format, FormatType, NoContent, text::Text,
    };
    use crate::endpoint::{NoPolicy, NoRoute};
    use crate::pagination::NoPagination;
    use std::convert::Infallible;

    struct TestCx;
    impl ClientContext for TestCx {
        type Vars = ();
        const SCHEME: Scheme = Scheme::HTTPS;
        const DOMAIN: &'static str = "example.com";
    }

    struct BinaryEncoding;
    impl ContentType for BinaryEncoding {
        const CONTENT_TYPE: &'static str = "application/octet-stream";
    }
    impl FormatType for BinaryEncoding {
        const FORMAT_TYPE: Format = Format::Binary;
    }
    impl Encodes<Vec<u8>> for BinaryEncoding {
        type Error = Infallible;
        fn encode(output: &Vec<u8>) -> Result<Bytes, Self::Error> {
            Ok(Bytes::copy_from_slice(output.as_slice()))
        }
    }

    struct Ep {
        body: Vec<u8>,
    }

    struct EpBody;
    impl BodyPart<Ep> for EpBody {
        type Body = Vec<u8>;
        type Enc = BinaryEncoding;
        fn body(ep: &Ep) -> Option<&Self::Body> {
            Some(&ep.body)
        }
    }

    impl Endpoint<TestCx> for Ep {
        const METHOD: http::Method = http::Method::POST;
        type Route = NoRoute;
        type Policy = NoPolicy;
        type Pagination = NoPagination;
        type Body = EpBody;
        type Response = crate::endpoint::Decoded<Text, String>;
    }

    #[test]
    fn debug_preview_uses_request_encoder_and_response_decoder_formats() {
        // Request: binary => base64
        let req = Bytes::from_static(&[0x00, 0x01, 0x02]);
        let req_s = format_request_body_for_debug::<TestCx, Ep>(&req, 1024);
        assert_eq!(req_s, "AAEC");

        // Response: text => UTF-8
        let resp = Bytes::from_static(b"hello");
        let resp_s = format_response_body_for_debug::<TestCx, Ep>(&resp, 1024);
        assert_eq!(resp_s, "hello");

        // sanity: NoContentEncoding is text-format (empty)
        let empty = Bytes::new();
        let s = crate::codec::format_debug_body::<NoContent>(&empty, 1024);
        assert_eq!(s, "");
    }
}
