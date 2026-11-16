use concord::http::HeaderValue;
use concord::reqwest::{Request, Response};
use concord::sign::{body_bytes, check_origin, SignError};
use concord::transport::{TransportError, TryInfo};
use concord::{Base, Middleware, Next, Secret};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Proof {
    origin: Base,
    key: Secret,
}

impl Proof {
    pub fn new(origin: Base, key: Secret) -> Self {
        Self { origin, key }
    }

    fn sign(&self, request: &mut Request) -> Result<(), SignError> {
        check_origin(request, &self.origin)?; // before exposing the key
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(SignError::failed)?
            .as_secs();
        let digest = hex::encode(Sha256::digest(body_bytes(request)?));
        let text = format!(
            "{}\n{}\n{}\n{}\n{}",
            seconds,
            request.method(),
            request.url().path(),
            request.url().query().unwrap_or(""),
            digest
        );
        let key = self.key.expose();
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_err(SignError::failed)?;
        mac.update(text.as_bytes());
        let mut proof = HeaderValue::from_str(&hex::encode(mac.finalize().into_bytes()))?;
        proof.set_sensitive(true);
        let timestamp = HeaderValue::from_str(&seconds.to_string())?;
        request
            .headers_mut()
            .try_insert("x-proof-time", timestamp)
            .map_err(SignError::failed)?;
        request
            .headers_mut()
            .try_insert("x-proof", proof)
            .map_err(SignError::failed)?;
        Ok(())
    }
}

impl Middleware for Proof {
    async fn handle(
        &self,
        mut request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        self.sign(&mut request)
            .map_err(|cause| TransportError::blocked().with_source(cause))?;
        next.send(request, info).await
    }
}

concord::api! {
    pub client Vault(origin: Base, key: Secret) {
        base origin;
        middleware Proof::new(origin.clone(), key.clone());
        GET balance() -> String { path "balance"; read concord::Text; }
    }
}
