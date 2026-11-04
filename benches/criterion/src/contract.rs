//! Untimed observations of the outputs of the registered consuming routines.
//!
//! Runners do not call this during measurement. Tests use the very same input/run closures,
//! with fresh inputs, rather than maintaining a second set of benchmark call sites.

use std::collections::BTreeMap;
use std::time::Duration;

use bytes::Bytes;

use crate::payload::Payload;

/// Prepared native state. Header names are canonicalized; values retain multiplicity,
/// same-name order and sensitivity. There is no cross-name HeaderMap ordering promise.
#[derive(Debug, PartialEq)]
pub struct Prepared {
    method: reqwest::Method,
    url: String,
    headers: BTreeMap<String, Vec<(Vec<u8>, bool)>>,
    body: Option<Vec<u8>>,
    version: reqwest::Version,
    timeout: Option<Duration>,
}

impl Prepared {
    /// Observe buffered request bodies without consuming them. A streaming fixture must be
    /// reconstructed and collected under its own controlled test, never treated as empty.
    pub fn observe(request: &reqwest::Request) -> Self {
        let headers = request
            .headers()
            .keys()
            .map(|name| {
                let values = request
                    .headers()
                    .get_all(name)
                    .iter()
                    .map(|value| (value.as_bytes().to_vec(), value.is_sensitive()))
                    .collect();
                (name.as_str().to_owned(), values)
            })
            .collect();
        Self {
            method: request.method().clone(),
            url: request.url().as_str().to_owned(),
            headers,
            body: request.body().map(|body| {
                body.as_bytes()
                    .expect("registered prepared bodies must be buffered; reconstruct streams")
                    .to_vec()
            }),
            version: request.version(),
            timeout: request.timeout().copied(),
        }
    }
}

/// The stage-specific observable output; decoded data is compared as its actual Rust value.
#[derive(Debug, PartialEq)]
pub enum Observation {
    Prepared(Prepared),
    Json(Payload),
    Text(String),
    Bytes(Bytes),
}

/// Outputs accepted by the existing scenario runners. Observation is exclusively untimed.
pub trait Observe {
    fn observe(&self) -> Observation;
}

impl<T> Observe for concord::Prepared<T> {
    fn observe(&self) -> Observation {
        Observation::Prepared(Prepared::observe(self.request()))
    }
}

impl Observe for reqwest::Request {
    fn observe(&self) -> Observation {
        Observation::Prepared(Prepared::observe(self))
    }
}

impl Observe for Payload {
    fn observe(&self) -> Observation {
        Observation::Json(self.clone())
    }
}

impl Observe for String {
    fn observe(&self) -> Observation {
        Observation::Text(self.clone())
    }
}

impl Observe for Bytes {
    fn observe(&self) -> Observation {
        Observation::Bytes(self.clone())
    }
}
