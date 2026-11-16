use base64::{engine::general_purpose::STANDARD, Engine};
use concord::http::HeaderValue;
use concord::sign::{body_bytes, check_origin, CanonicalPath, SigV4, SignError};
use concord::transport::{TransportError, TryInfo};
use concord::{Base, Middleware, Next};
use md5::{Digest, Md5};

/// Checks the configured bucket origins, adds the bulk-delete checksum, and delegates signing.
/// The list of buckets uses the global origin and `us-east-1`.
pub struct Signed {
    credentials: Credentials,
    region: String,
    origins: Vec<Base>,
    signer: std::sync::OnceLock<SigV4>,
}

impl Signed {
    pub fn new(credentials: Credentials, region: String, origins: Vec<Base>) -> Self {
        Self { credentials, region, origins, signer: std::sync::OnceLock::new() }
    }

    fn sign(&self, request: &mut concord::reqwest::Request, endpoint: &str) -> Result<(), SignError> {
        let global: Base = "https://s3.amazonaws.com".parse().map_err(SignError::failed)?;
        let (origin, region) = if endpoint == "object_store.list_buckets" {
            check_origin(request, &global)?;
            (&global, "us-east-1")
        } else {
            let origin = self.origins.iter().find(|base| check_origin(request, base).is_ok())
                .ok_or(SignError::WrongOrigin)?;
            (origin, self.region.as_str())
        };
        if endpoint == "object_store.bucket.delete_objects" {
            let digest = STANDARD.encode(Md5::digest(body_bytes(request)?));
            request.headers_mut().try_insert("content-md5", HeaderValue::from_str(&digest)?)
                .map_err(SignError::failed)?;
        }
        // The origin was selected from configuration, never from the outgoing URL.
        self.signer.get_or_init(||
            SigV4::new(origin, self.credentials.clone(), region, "s3", CanonicalPath::Preserve)
                .allow_unsigned_payload()
        ).sign(request, region)
    }
}

impl Middleware for Signed {
    async fn handle(&self, mut request: concord::reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<concord::reqwest::Response, TransportError>
    {
        self.sign(&mut request, info.endpoint())
            .map_err(|error| TransportError::blocked().with_source(error))?;
        next.send(request, info).await
    }
}
