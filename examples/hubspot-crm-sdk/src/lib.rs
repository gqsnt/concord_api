//! A bounded HubSpot CRM contacts client. See the package README for safety and persistence limits.
use concord::{
    Form, Json,
    reqwest::{
        self,
        header::{AUTHORIZATION, HeaderValue},
    },
};
use concord_auth::{Token, TokenCache};
use concord_pagination::{Page, Pages, Resume};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};

const BODY_LIMIT: usize = 1024 * 1024;

concord::api! {
    /// The CRM wire declaration. Prefer [`HubSpot`] for coordinated refresh and error semantics.
    pub client CrmApi(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://api.hubapi.com";
        format Json;
        limit BODY_LIMIT;
        scope contacts {
            path "crm/v3/objects/contacts";
            /// Search one page.
            POST search(body request: SearchRequest) -> SearchResponse { path "search"; }
            /// Create a contact once.
            POST create(body request: CreateContact) -> Contact;
            /// Get a contact by ID.
            GET get(id: String) -> Contact { path "{id}"; }
        }
    }
}
concord::api! {
    /// OAuth wire declaration. Refresh credentials are form body values, never URL parameters.
    pub client OAuthApi {
        base "https://api.hubspot.com";
        response Json;
        limit BODY_LIMIT;
        scope oauth {
            path "oauth/2026-09";
            /// Exchange a refresh token.
            POST refresh(body request: RefreshRequest) -> RefreshResponse {
                path "token";
                request Form;
            }
        }
    }
}

/// A contact with arbitrary selected HubSpot properties.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Contact {
    /// HubSpot contact identifier.
    pub id: String,
    /// Selected properties (values may be null).
    pub properties: BTreeMap<String, Option<String>>,
}
/// Properties for a new contact.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CreateContact {
    /// Property names and values.
    pub properties: BTreeMap<String, String>,
}
/// A filter in a search group.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    /// Property to inspect.
    pub property_name: String,
    /// HubSpot operator, for example `EQ`.
    pub operator: String,
    /// Comparison value.
    pub value: String,
}
/// A group of AND filters; groups are ORed by HubSpot.
#[derive(Clone, Debug, Serialize)]
pub struct FilterGroup {
    /// Filters in the group.
    pub filters: Vec<Filter>,
}
/// Search parameters; `after` is overwritten by the traversal with the provider cursor.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// Filter groups.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub filter_groups: Vec<FilterGroup>,
    /// Free-text search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Selected property names.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub properties: Vec<String>,
    /// Page size, up to 200.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Provider continuation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}
/// Search result returned by HubSpot.
#[derive(Clone, Debug, Deserialize)]
pub struct SearchResponse {
    /// Count reported by HubSpot.
    pub total: u64,
    /// Contacts on this page.
    pub results: Vec<Contact>,
    /// Optional next-page information.
    pub paging: Option<Paging>,
}
/// Search paging envelope.
#[derive(Clone, Debug, Deserialize)]
pub struct Paging {
    /// Next page if present.
    pub next: Option<Next>,
}
/// Search continuation.
#[derive(Clone, Debug, Deserialize)]
pub struct Next {
    /// Opaque cursor; never follow the optional provider link as a URL.
    pub after: String,
}
/// OAuth form. Its debug output never contains credentials.
#[derive(Clone, Serialize)]
pub struct RefreshRequest {
    grant_type: &'static str,
    client_id: String,
    client_secret: String,
    refresh_token: String,
}
impl fmt::Debug for RefreshRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RefreshRequest(<redacted>)")
    }
}
/// OAuth response, whose debug output is redacted.
#[derive(Deserialize)]
pub struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
}
impl fmt::Debug for RefreshResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RefreshResponse(<redacted>)")
    }
}
/// An SDK failure. Formatting never includes request values or response bodies.
#[derive(Debug)]
pub enum Error {
    /// Concord transport, status, media, size, decoding or construction failure.
    Concord(concord::Error),
    /// Refresh failed; `status` is present when HubSpot returned an HTTP error.
    Refresh {
        /// HTTP status if available.
        status: Option<u16>,
        /// A response or transport failure may have consumed or rotated the refresh token.
        uncertain: bool,
    },
    /// A prior refresh may have rotated the token without a usable response; reauthorize.
    RefreshUncertain,
    /// A create may have reached HubSpot. Reconcile before attempting another create.
    CreateUncertain,
    /// An SDK input or response continuation was invalid.
    Invalid(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => write!(f, "HubSpot request: {e}"),
            Self::Refresh { status, uncertain } => write!(
                f,
                "HubSpot refresh failed (status {status:?}, uncertain {uncertain})"
            ),
            Self::RefreshUncertain => {
                f.write_str("HubSpot refresh outcome uncertain; recover credentials")
            }
            Self::CreateUncertain => {
                f.write_str("HubSpot contact creation outcome uncertain; reconcile before retrying")
            }
            Self::Invalid(s) => write!(f, "HubSpot: {s}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::Concord(e)
    }
}

#[derive(Clone, Debug)]
struct RefreshFailure {
    status: Option<u16>,
    uncertain: bool,
}
struct Secrets {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    uncertain: bool,
}
struct Inner {
    http: reqwest::Client,
    crm_base: String,
    oauth_base: String,
    secrets: Mutex<Secrets>,
    cache: TokenCache<(), RefreshFailure>,
}
/// One HubSpot installation. Clones coordinate refresh in this process.
#[derive(Clone)]
pub struct HubSpot {
    inner: Arc<Inner>,
}
impl fmt::Debug for HubSpot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HubSpot(<redacted>)")
    }
}
impl HubSpot {
    /// Construct one installation. The supplied HTTP client must disable redirects and automatic
    /// retries, and use a finite timeout. Store and coordinate refresh tokens externally if needed.
    pub fn new(
        http: reqwest::Client,
        client_id: String,
        client_secret: String,
        refresh_token: String,
    ) -> Result<Self, Error> {
        if client_id.is_empty() || client_secret.is_empty() || refresh_token.is_empty() {
            return Err(Error::Invalid("empty OAuth credential"));
        }
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                crm_base: "https://api.hubapi.com".into(),
                oauth_base: "https://api.hubspot.com".into(),
                secrets: Mutex::new(Secrets {
                    client_id,
                    client_secret,
                    refresh_token,
                    uncertain: false,
                }),
                cache: TokenCache::new(
                    NonZeroUsize::new(1).expect("nonzero"),
                    Duration::from_secs(30),
                ),
            }),
        })
    }
    /// Set the common API base before cloning; intended for trusted loopback fixture servers.
    pub fn with_base_url(mut self, url: &str) -> Result<Self, Error> {
        let inner = Arc::get_mut(&mut self.inner)
            .ok_or(Error::Invalid("base cannot change after cloning"))?;
        OAuthApi::new(inner.http.clone()).with_base_url(url)?;
        inner.crm_base = url.to_owned();
        inner.oauth_base = url.to_owned();
        Ok(self)
    }
    /// Return the latest known refresh token for secure persistence. It is secret material.
    /// An uncertain refresh returns an error: the value may no longer be current.
    pub fn refresh_token(&self) -> Result<String, Error> {
        let state = self.inner.secrets.lock().expect("secrets mutex");
        if state.uncertain {
            return Err(Error::RefreshUncertain);
        }
        Ok(state.refresh_token.clone())
    }
    async fn token(&self) -> Result<HeaderValue, Error> {
        let inner = self.inner.clone();
        let snapshot = self
            .inner
            .cache
            .get_or_refresh((), move || async move {
                let request = {
                    let mut s = inner.secrets.lock().expect("secrets mutex");
                    if s.uncertain {
                        return Err(RefreshFailure {
                            status: None,
                            uncertain: true,
                        });
                    }
                    s.uncertain = true;
                    RefreshRequest {
                        grant_type: "refresh_token",
                        client_id: s.client_id.clone(),
                        client_secret: s.client_secret.clone(),
                        refresh_token: s.refresh_token.clone(),
                    }
                };
                let api = OAuthApi::new(inner.http.clone())
                    .with_base_url(&inner.oauth_base)
                    .map_err(|_| RefreshFailure {
                        status: None,
                        uncertain: true,
                    })?;
                let result = api.oauth().refresh(request).send().await;
                match result {
                    Ok(response) => {
                        let mut s = inner.secrets.lock().expect("secrets mutex");
                        if let Some(replacement) = response.refresh_token {
                            if replacement.is_empty() {
                                return Err(RefreshFailure {
                                    status: None,
                                    uncertain: true,
                                });
                            }
                            s.refresh_token = replacement;
                        }
                        if !response.token_type.eq_ignore_ascii_case("bearer")
                            || response.expires_in <= 30
                        {
                            return Err(RefreshFailure {
                                status: None,
                                uncertain: true,
                            });
                        }
                        let header =
                            concord_auth::bearer(&response.access_token).map_err(|_| {
                                RefreshFailure {
                                    status: None,
                                    uncertain: true,
                                }
                            })?;
                        s.uncertain = false;
                        Ok(Token::new(header, Duration::from_secs(response.expires_in)))
                    }
                    Err(e) => {
                        let status = e.status().map(|s| s.as_u16());
                        // Treat explicit OAuth credential refusals as definite; other outcomes are ambiguous.
                        let uncertain = !status.is_some_and(|s| matches!(s, 400 | 401 | 403));
                        inner.secrets.lock().expect("secrets mutex").uncertain = uncertain;
                        Err(RefreshFailure { status, uncertain })
                    }
                }
            })
            .await
            .map_err(|e| match e {
                concord_auth::RefreshError::Acquire(e) => Error::Refresh {
                    status: e.status,
                    uncertain: e.uncertain,
                },
                _ => Error::Refresh {
                    status: None,
                    uncertain: true,
                },
            })?;
        Ok(snapshot.header().clone())
    }
    async fn api(&self) -> Result<CrmApi, Error> {
        let authorization = self.token().await?;
        Ok(
            CrmApi::new(self.inner.http.clone(), CrmApiConfig { authorization })
                .with_base_url(&self.inner.crm_base)?,
        )
    }
    /// Retrieve a contact by ID.
    pub async fn get(&self, id: &str) -> Result<Contact, Error> {
        if id.is_empty() {
            return Err(Error::Invalid("empty contact ID"));
        }
        Ok(self.api().await?.contacts().get(id).send().await?)
    }
    /// Create exactly one request. A transport, invalid success body, or server error is uncertain.
    pub async fn create(&self, contact: CreateContact) -> Result<Contact, Error> {
        let result = self.api().await?.contacts().create(contact).send().await;
        match result {
            Ok(contact) => Ok(contact),
            Err(e) if e.kind() == concord::ErrorKind::Build => Err(e.into()),
            Err(e)
                if e.status()
                    .is_some_and(|s| s.is_client_error() && s.as_u16() != 408) =>
            {
                Err(e.into())
            }
            Err(_) => Err(Error::CreateUncertain),
        }
    }
    /// Search one page, preserving the provider's total as page metadata.
    pub async fn search_page(
        &self,
        mut request: SearchRequest,
        after: Option<String>,
    ) -> Result<Page<Contact, String, u64>, Error> {
        if request.limit.is_some_and(|n| !(1..=200).contains(&n)) {
            return Err(Error::Invalid("search limit must be 1..=200"));
        }
        request.after = after.clone();
        let response = self.api().await?.contacts().search(request).send().await?;
        let next = response.paging.and_then(|p| p.next).map(|n| n.after);
        if next.as_deref().is_some_and(|s| s.is_empty()) || (next.is_some() && next == after) {
            return Err(Error::Invalid("invalid or unchanged search cursor"));
        }
        Ok(Page {
            items: response.results,
            next,
            meta: response.total,
        })
    }
    /// Traverse search results with explicit completion, errors, and optional page cap.
    pub fn search(
        &self,
        request: SearchRequest,
        from: Resume<String>,
    ) -> Pages<Contact, String, u64, Error> {
        let client = self.clone();
        Pages::new(from, move |after| {
            let client = client.clone();
            let request = request.clone();
            async move { client.search_page(request, after).await }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oauth_default_destination_is_documented_host() {
        let request = RefreshRequest {
            grant_type: "refresh_token",
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: "refresh".into(),
        };
        let prepared = OAuthApi::new(reqwest::Client::new())
            .oauth()
            .refresh(request)
            .prepare()
            .unwrap();
        assert_eq!(
            prepared.request().url().as_str(),
            "https://api.hubspot.com/oauth/2026-09/token"
        );
    }
}
