//! S3 baseline: search with 8 optional query params (repeated `tag`, defaulted
//! `limit=20`), `POST` JSON create (201), `DELETE` (204). Bearer token header.
use reqwest::header::{AUTHORIZATION, HeaderValue};
use reqwest::{Client, Method, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
pub enum Sort {
    Relevance,
    PriceAsc,
    PriceDesc,
    Newest,
}

impl Sort {
    fn as_str(self) -> &'static str {
        match self {
            Sort::Relevance => "relevance",
            Sort::PriceAsc => "price_asc",
            Sort::PriceDesc => "price_desc",
            Sort::Newest => "newest",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub price_cents: u32,
}

#[derive(Debug, Deserialize)]
pub struct SearchPage {
    pub items: Vec<Item>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NewItem {
    pub name: String,
    pub price_cents: u32,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ShopApi {
    http: Client,
    base: Url,
    token: HeaderValue,
}

impl ShopApi {
    pub fn new(
        http: Client,
        base: Url,
        token: &str,
    ) -> Result<Self, reqwest::header::InvalidHeaderValue> {
        let mut token = HeaderValue::from_str(&format!("Bearer {token}"))?;
        token.set_sensitive(true);
        Ok(Self { http, base, token })
    }

    fn request(&self, method: Method, segments: &[&str]) -> RequestBuilder {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("http(s) base")
            .pop_if_empty()
            .extend(segments);
        self.http
            .request(method, url)
            .header(AUTHORIZATION, self.token.clone())
    }

    pub fn search(&self) -> SearchItems<'_> {
        SearchItems {
            api: self,
            q: None,
            tags: Vec::new(),
            category: None,
            min_price: None,
            max_price: None,
            in_stock: None,
            sort: None,
            cursor: None,
            limit: 20,
        }
    }

    pub async fn create_item(&self, item: &NewItem) -> reqwest::Result<Item> {
        let response = self
            .request(Method::POST, &["items"])
            .json(item)
            .send()
            .await?;
        response.error_for_status()?.json().await
    }

    pub async fn delete_item(&self, id: &str) -> reqwest::Result<()> {
        self.request(Method::DELETE, &["items", id])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
#[must_use = "call .send().await"]
pub struct SearchItems<'a> {
    api: &'a ShopApi,
    q: Option<String>,
    tags: Vec<String>,
    category: Option<String>,
    min_price: Option<u32>,
    max_price: Option<u32>,
    in_stock: Option<bool>,
    sort: Option<Sort>,
    cursor: Option<String>,
    limit: u32,
}

impl SearchItems<'_> {
    pub fn q(mut self, q: impl Into<String>) -> Self {
        self.q = Some(q.into());
        self
    }
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
    pub fn category(mut self, c: impl Into<String>) -> Self {
        self.category = Some(c.into());
        self
    }
    pub fn min_price(mut self, cents: u32) -> Self {
        self.min_price = Some(cents);
        self
    }
    pub fn max_price(mut self, cents: u32) -> Self {
        self.max_price = Some(cents);
        self
    }
    pub fn in_stock(mut self, in_stock: bool) -> Self {
        self.in_stock = Some(in_stock);
        self
    }
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort = Some(sort);
        self
    }
    pub fn cursor(mut self, cursor: Option<String>) -> Self {
        self.cursor = cursor;
        self
    }
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Escape hatch: the native request with every query pair applied.
    pub fn into_request(self) -> RequestBuilder {
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if let Some(q) = self.q {
            pairs.push(("q", q));
        }
        pairs.extend(self.tags.into_iter().map(|t| ("tag", t)));
        if let Some(c) = self.category {
            pairs.push(("category", c));
        }
        if let Some(p) = self.min_price {
            pairs.push(("min_price", p.to_string()));
        }
        if let Some(p) = self.max_price {
            pairs.push(("max_price", p.to_string()));
        }
        if let Some(s) = self.in_stock {
            pairs.push(("in_stock", s.to_string()));
        }
        if let Some(s) = self.sort {
            pairs.push(("sort", s.as_str().to_owned()));
        }
        if let Some(c) = self.cursor {
            pairs.push(("cursor", c));
        }
        pairs.push(("limit", self.limit.to_string()));
        self.api
            .request(Method::GET, &["items", "search"])
            .query(&pairs)
    }

    pub async fn send(self) -> reqwest::Result<SearchPage> {
        self.into_request()
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }
}
