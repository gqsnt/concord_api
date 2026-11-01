//! S1 baseline: base URL, `GET /health` -> JSON, `GET /items/{id}` -> JSON.
use reqwest::{Client, Url};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Health {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct SimpleApi {
    http: Client,
    base: Url,
}

impl SimpleApi {
    pub const DEFAULT_BASE: &'static str = "https://api.example.com/v1/";

    pub fn new() -> Self {
        Self::with_client(
            Client::new(),
            Url::parse(Self::DEFAULT_BASE).expect("valid default base"),
        )
    }

    /// Bring your own `reqwest::Client` (proxy, TLS, timeouts) and base (staging, mock server).
    pub fn with_client(http: Client, base: Url) -> Self {
        Self { http, base }
    }

    /// Appends percent-encoded segments to the base path.
    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("http(s) base")
            .pop_if_empty()
            .extend(segments);
        url
    }

    pub async fn health(&self) -> reqwest::Result<Health> {
        let response = self.http.get(self.url(&["health"])).send().await?;
        response.error_for_status()?.json().await
    }

    pub async fn item(&self, id: &str) -> reqwest::Result<Item> {
        let response = self.http.get(self.url(&["items", id])).send().await?;
        response.error_for_status()?.json().await
    }
}

impl Default for SimpleApi {
    fn default() -> Self {
        Self::new()
    }
}
