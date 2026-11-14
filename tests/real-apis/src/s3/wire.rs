use aws_sigv4::http_request::{
    sign, PayloadChecksumKind, PercentEncodingMode, SignableBody, SignableRequest, SigningSettings,
    UriPathNormalizationMode,
};
use aws_sigv4::sign::v4;
use base64::{engine::general_purpose::STANDARD, Engine};
use concord::http::{HeaderName, HeaderValue};
use concord::read::{Incoming, ReadError, Reader};
use concord::transport::{TransportError, TryInfo};
use concord::{reqwest, Body, Middleware, Next};
use md5::{Digest, Md5};
use serde::{de::DeserializeOwned, Serialize};
use std::time::SystemTime;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// `read Xml;`: the body, at most 10 MiB, read by `quick-xml`.
#[derive(Clone, Copy, Debug)]
pub struct Xml;

impl<T: DeserializeOwned + Send> Reader<T> for Xml {
    async fn read(&self, body: Incoming) -> Result<T, ReadError> {
        let bytes = body.bytes(10 * concord::MIB).await?;
        quick_xml::de::from_reader(&bytes[..])
            .map_err(|error| ReadError::not_the_type(bytes).with_details(error.to_string()))
    }
}

/// `body xml(batch);`
pub fn xml<T: Serialize>(value: &T) -> Result<Body, quick_xml::SeError> {
    Ok(Body::new(quick_xml::se::to_string(value)?, "application/xml"))
}

pub fn create_bucket_body(value: &Option<CreateBucketConfiguration>) -> Result<Body, quick_xml::SeError> {
    match value {
        Some(configuration) => xml(configuration),
        None => Ok(Body::new("", "application/xml")),
    }
}

/// `middleware Signed::new(credentials.clone(), region.clone());`: AWS Signature Version 4, on
/// every attempt. The list of buckets is signed for `us-east-1`; everything else for the region.
/// A delete of many keys also needs a `content-md5` of its body: the description cannot make a
/// header from the bytes of a body, so the signer adds it, before it signs.
pub struct Signed {
    credentials: Credentials,
    region: String,
}

impl Signed {
    pub fn new(credentials: Credentials, region: String) -> Self {
        Signed { credentials, region }
    }

    fn sign(&self, request: &mut reqwest::Request, endpoint: &str) -> Result<(), BoxError> {
        if endpoint == "object_store.bucket.delete_objects" {
            let bytes = request.body().and_then(|body| body.as_bytes()).ok_or("no body in memory")?;
            let digest = STANDARD.encode(Md5::digest(bytes));
            request.headers_mut().insert("content-md5", HeaderValue::from_str(&digest)?);
        }
        let identity = aws_credential_types::Credentials::new(
            &self.credentials.access_key_id,
            self.credentials.secret_access_key.expose().to_string(),
            self.credentials.session_token.as_ref().map(|token| token.expose().to_string()),
            None,
            "concord",
        )
        .into();
        let region = if endpoint == "object_store.list_buckets" { "us-east-1" } else { &self.region };
        let mut settings = SigningSettings::default();
        settings.percent_encoding_mode = PercentEncodingMode::Single;
        settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
        settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
        let params = v4::SigningParams::builder()
            .identity(&identity)
            .region(region)
            .name("s3")
            .time(SystemTime::now())
            .settings(settings)
            .build()?
            .into();
        let payload = match request.body() {
            None => SignableBody::Bytes(&[]),
            Some(body) => match body.as_bytes() {
                Some(bytes) => SignableBody::Bytes(bytes),
                None => SignableBody::UnsignedPayload,     // a stream: sent once, over HTTPS
            },
        };
        let headers = request.headers().iter()
            .map(|(name, value)| Ok((name.as_str(), value.to_str()?)))
            .collect::<Result<Vec<_>, concord::http::header::ToStrError>>()?;
        let signable = SignableRequest::new(
            request.method().as_str(), request.url().as_str(), headers.into_iter(), payload,
        )?;
        let (instructions, _) = sign(signable, &params)?.into_parts();
        for (name, value) in instructions.headers() {
            let mut value = HeaderValue::from_str(value)?;
            value.set_sensitive(name == "authorization" || name == "x-amz-security-token");
            request.headers_mut().insert(HeaderName::from_bytes(name.as_bytes())?, value);
        }
        Ok(())
    }
}

impl Middleware for Signed {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        self.sign(&mut request, info.endpoint())
            .map_err(|error| TransportError::blocked().with_source(error))?;
        next.send(request, info).await
    }
}
