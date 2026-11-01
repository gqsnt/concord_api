//! S2 baseline: `https://{region}.api.example.com/v2/orgs/{org}/...`, sensitive
//! `X-Api-Key`, shared `Accept`, `X-Api-Version` defaulted and overridable.
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use reqwest::{Client, RequestBuilder, Url};
use serde::Deserialize;

pub const DEFAULT_API_VERSION: &str = "2024-06-01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Eu,
    Na,
    Ap,
}

impl Region {
    fn label(self) -> &'static str {
        match self {
            Region::Eu => "eu",
            Region::Na => "na",
            Region::Ap => "ap",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Status {
    pub healthy: bool,
}

#[derive(Debug, Deserialize)]
pub struct Product {
    pub id: String,
    pub title: String,
}

/// `Debug` is safe: the key `HeaderValue` is marked sensitive and prints as `Sensitive`.
#[derive(Clone, Debug)]
pub struct CatalogApi {
    http: Client,
    base: Url,
    api_key: HeaderValue,
    api_version: HeaderValue,
}

#[derive(Debug)]
pub enum BuildError {
    InvalidBase(url::ParseError),
    InvalidHeader(reqwest::header::InvalidHeaderValue),
}

impl CatalogApi {
    pub fn new(api_key: &str) -> Result<Self, BuildError> {
        Self::with_client(Client::new(), "https://api.example.com/v2/", api_key)
    }

    pub fn with_client(http: Client, base: &str, api_key: &str) -> Result<Self, BuildError> {
        let base = Url::parse(base).map_err(BuildError::InvalidBase)?;
        let mut api_key = HeaderValue::from_str(api_key).map_err(BuildError::InvalidHeader)?;
        api_key.set_sensitive(true);
        let api_version = HeaderValue::from_static(DEFAULT_API_VERSION);
        Ok(Self {
            http,
            base,
            api_key,
            api_version,
        })
    }

    /// Returns a copy that sends another `X-Api-Version`.
    pub fn with_api_version(mut self, version: &str) -> Result<Self, BuildError> {
        self.api_version = HeaderValue::from_str(version).map_err(BuildError::InvalidHeader)?;
        Ok(self)
    }

    pub fn regional(&self, region: Region) -> Regional<'_> {
        Regional { api: self, region }
    }

    /// Escape hatch: a native request with host, path and shared headers applied.
    pub fn request(
        &self,
        method: reqwest::Method,
        region: Region,
        segments: &[&str],
    ) -> RequestBuilder {
        let mut url = self.base.clone();
        let host = format!(
            "{}.{}",
            region.label(),
            self.base.host_str().expect("base has host")
        );
        url.set_host(Some(&host)).expect("valid regional host");
        url.path_segments_mut()
            .expect("http(s) base")
            .pop_if_empty()
            .extend(segments);
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert("x-api-key", self.api_key.clone());
        headers.insert("x-api-version", self.api_version.clone());
        self.http.request(method, url).headers(headers)
    }
}

async fn json<T: serde::de::DeserializeOwned>(request: RequestBuilder) -> reqwest::Result<T> {
    request.send().await?.error_for_status()?.json().await
}

#[derive(Clone, Copy, Debug)]
pub struct Regional<'a> {
    api: &'a CatalogApi,
    region: Region,
}

impl<'a> Regional<'a> {
    pub async fn status(&self) -> reqwest::Result<Status> {
        json(
            self.api
                .request(reqwest::Method::GET, self.region, &["status"]),
        )
        .await
    }

    pub fn org(&self, org: &'a str) -> Org<'a> {
        Org {
            regional: *self,
            org,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Org<'a> {
    regional: Regional<'a>,
    org: &'a str,
}

impl Org<'_> {
    fn get(&self, rest: &[&str]) -> RequestBuilder {
        let mut segments = vec!["orgs", self.org, "catalog"];
        segments.extend_from_slice(rest);
        self.regional
            .api
            .request(reqwest::Method::GET, self.regional.region, &segments)
    }

    pub async fn products(&self) -> reqwest::Result<Vec<Product>> {
        json(self.get(&["products"])).await
    }

    pub async fn product(&self, id: &str) -> reqwest::Result<Product> {
        json(self.get(&["products", id])).await
    }
}
