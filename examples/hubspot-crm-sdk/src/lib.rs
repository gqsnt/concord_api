//! A bounded HubSpot CRM contacts client. See the package README for safety and persistence limits.
use concord::{
    Delivery, Form, Json,
    reqwest::{
        self,
        header::{AUTHORIZATION, HeaderValue},
    },
};
use concord_auth::{Token, TokenCache};
use concord_pagination::{Page, Pages, Resume};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::{BTreeMap, BTreeSet},
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
            #[read] POST search(body request: SearchRequest) -> SearchResponse { path "search"; }
            /// Create a contact once.
            POST create(body request: CreateContact) -> Contact;
            /// Get a contact by ID.
            GET get(id: String) -> Contact { path "{id}"; }
            /// Read several contacts by record ID; a `200` or `207 Multi-Status` body.
            #[read] POST batch_read(body request: BatchReadRequest) -> BatchReadResponse {
                path "batch/read";
            }
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
/// Largest batch HubSpot documents for CRM object batch operations.
pub const BATCH_READ_MAX: usize = 100;

/// Contacts to read by record ID in one batch request.
#[derive(Clone, Debug, Default)]
pub struct BatchReadRequest {
    /// Record IDs: 1..=[`BATCH_READ_MAX`], each non-empty and distinct.
    pub ids: Vec<String>,
    /// Selected property names; empty selects HubSpot's default properties.
    pub properties: Vec<String>,
}
impl BatchReadRequest {
    fn validate(&self) -> Result<(), Error> {
        if self.ids.is_empty() || self.ids.len() > BATCH_READ_MAX {
            return Err(Error::Invalid(
                "batch read ID count outside 1..=BATCH_READ_MAX",
            ));
        }
        let mut seen = BTreeSet::new();
        if !self.ids.iter().all(|id| !id.is_empty() && seen.insert(id)) {
            return Err(Error::Invalid(
                "empty or duplicate contact ID in batch read",
            ));
        }
        Ok(())
    }
}
impl Serialize for BatchReadRequest {
    /// `{"inputs":[{"id":..}],"properties":[..],"propertiesWithHistory":[]}`: lookup by record
    /// ID (no `idProperty`); the documented required history list is always empty.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Input<'a> {
            id: &'a str,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            inputs: Vec<Input<'a>>,
            properties: &'a [String],
            properties_with_history: [&'a str; 0],
        }
        Wire {
            inputs: self.ids.iter().map(|id| Input { id }).collect(),
            properties: &self.properties,
            properties_with_history: [],
        }
        .serialize(serializer)
    }
}
/// Batch read body as HubSpot sends it. [`HubSpot::batch_read`] attributes it per requested ID.
#[derive(Clone, Debug, Deserialize)]
pub struct BatchReadResponse {
    /// Contacts found, in no documented order.
    pub results: Vec<Contact>,
    /// Item errors of a `207 Multi-Status` response.
    #[serde(default)]
    pub errors: Vec<BatchError>,
}
/// One batch item error. Only the category and `context.ids` are retained: HubSpot's message,
/// links, sub-category, other context and error-instance `id` are never stored.
#[derive(Clone, Debug, Deserialize)]
pub struct BatchError {
    /// Provider category, for example `OBJECT_NOT_FOUND`.
    pub category: String,
    /// Contact IDs named by the error's `context.ids`.
    #[serde(default, rename = "context", deserialize_with = "context_ids")]
    pub ids: Vec<String>,
}
fn context_ids<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    struct Context {
        #[serde(default)]
        ids: Vec<String>,
    }
    Ok(Option::<Context>::deserialize(deserializer)?.map_or_else(Vec::new, |c| c.ids))
}
/// Why one requested ID has no contact. Formatting never includes a provider message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemError {
    /// HubSpot named this ID in a batch error with this category, e.g. `OBJECT_NOT_FOUND`.
    Provider {
        /// The provider's error category.
        category: String,
    },
    /// HubSpot's response named this ID neither as a result nor in an error.
    Unreported,
}
impl fmt::Display for ItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider { category } => write!(f, "HubSpot batch item failed ({category})"),
            Self::Unreported => f.write_str("HubSpot batch response did not report this ID"),
        }
    }
}
impl std::error::Error for ItemError {}
/// The outcome for one requested ID.
#[derive(Clone, Debug)]
pub struct BatchItem {
    /// The requested record ID.
    pub id: String,
    /// The contact, or why HubSpot returned none for this ID.
    pub result: Result<Contact, ItemError>,
}
/// A batch read attributed per requested ID.
#[derive(Clone, Debug)]
pub struct BatchRead {
    /// Exactly one outcome per requested ID, in request order.
    pub items: Vec<BatchItem>,
    /// Returned contacts whose ID was not requested (for example a merged contact returned under
    /// another ID, which HubSpot does not document); not attributed to any requested ID.
    pub unmatched: Vec<Contact>,
    /// Categories of item errors that named no requested ID.
    pub unattributed: Vec<String>,
}
impl BatchRead {
    /// The outcome for one requested ID.
    pub fn get(&self, id: &str) -> Option<&Result<Contact, ItemError>> {
        self.items.iter().find(|i| i.id == id).map(|i| &i.result)
    }
}
fn attribute(ids: &[String], response: BatchReadResponse) -> Result<BatchRead, Error> {
    let requested: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let mut found = BTreeMap::new();
    let mut unmatched = Vec::new();
    for contact in response.results {
        if !requested.contains(contact.id.as_str()) {
            unmatched.push(contact);
        } else if found.insert(contact.id.clone(), contact).is_some() {
            return Err(Error::Invalid("duplicate batch read result"));
        }
    }
    let mut failed = BTreeMap::new();
    let mut unattributed = Vec::new();
    for error in response.errors {
        let mut named = false;
        for id in error.ids {
            if requested.contains(id.as_str()) {
                if found.contains_key(&id) {
                    return Err(Error::Invalid("batch read reported a found ID as failed"));
                }
                named = true;
                failed.entry(id).or_insert_with(|| error.category.clone());
            }
        }
        if !named {
            unattributed.push(error.category);
        }
    }
    let items =
        ids.iter()
            .map(|id| BatchItem {
                id: id.clone(),
                result: found.remove(id).ok_or_else(|| {
                    failed.remove(id).map_or(ItemError::Unreported, |category| {
                        ItemError::Provider { category }
                    })
                }),
            })
            .collect();
    Ok(BatchRead {
        items,
        unmatched,
        unattributed,
    })
}

/// OAuth form. Its debug output never contains credentials.
#[derive(Clone, Serialize, concord::Debug)]
#[sensitive]
pub struct RefreshRequest {
    grant_type: &'static str,
    client_id: String,
    client_secret: String,
    refresh_token: String,
}

/// OAuth response, whose debug output is redacted.
#[derive(Deserialize, concord::Debug)]
#[sensitive]
pub struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
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
    // Shared with the acquisition future the cache stores, which must not own `Inner`.
    secrets: Arc<Mutex<Secrets>>,
    cache: TokenCache<(), RefreshFailure>,
}
/// CRM wrappers send `authorization: Bearer <access_token>` from the current OAuth token
/// snapshot, acquired before constructing each CRM operation.
///
/// One HubSpot installation. Clones coordinate refresh in this process.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct HubSpot {
    inner: Arc<Inner>,
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
                secrets: Arc::new(Mutex::new(Secrets {
                    client_id,
                    client_secret,
                    refresh_token,
                    uncertain: false,
                })),
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
        // The cache keeps this future while it is in flight: capture its inputs, not `Inner`.
        let secrets = self.inner.secrets.clone();
        let http = self.inner.http.clone();
        let oauth_base = self.inner.oauth_base.clone();
        let snapshot = self
            .inner
            .cache
            .get_or_refresh((), move || async move {
                let request = {
                    let mut s = secrets.lock().expect("secrets mutex");
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
                let api = OAuthApi::new(http)
                    .with_base_url(&oauth_base)
                    .map_err(|_| RefreshFailure {
                        status: None,
                        uncertain: true,
                    })?;
                let result = api.oauth().refresh(request).send().await;
                match result {
                    Ok(response) => {
                        let mut s = secrets.lock().expect("secrets mutex");
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
                        secrets.lock().expect("secrets mutex").uncertain = uncertain;
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
    /// Read several contacts by record ID in one request, sent once and never retried by the SDK.
    ///
    /// A `200` or `207 Multi-Status` response gives every requested ID either its contact or an
    /// [`ItemError`]: the category of a batch error whose `context.ids` names the ID, or
    /// [`ItemError::Unreported`]. A non-2xx status, timeout or undecodable body fails the whole
    /// call as [`Error::Concord`]; repeating it is safe but is the caller's decision. Invalid
    /// input is refused before any request; a response listing a requested ID twice, or as both
    /// found and failed, is [`Error::Invalid`].
    ///
    /// ```no_run
    /// # async fn example(sdk: concord_sdk_hubspot_crm::HubSpot) -> Result<(), concord_sdk_hubspot_crm::Error> {
    /// use concord_sdk_hubspot_crm::{BatchReadRequest, ItemError};
    /// let request = BatchReadRequest {
    ///     ids: vec!["101".into(), "102".into()],
    ///     properties: vec!["email".into()],
    /// };
    /// for item in sdk.batch_read(request).await?.items {
    ///     match item.result {
    ///         Ok(contact) => { let _email = contact.properties.get("email"); }
    ///         Err(ItemError::Provider { category }) if category == "OBJECT_NOT_FOUND" => {}
    ///         Err(_other) => {}
    ///     }
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn batch_read(&self, request: BatchReadRequest) -> Result<BatchRead, Error> {
        request.validate()?;
        let ids = request.ids.clone();
        let api = self.api().await?;
        let response = api.contacts().batch_read(request).send().await?;
        attribute(&ids, response)
    }
    /// Create once. Anything but a build failure or a definite client error is uncertain.
    pub async fn create(&self, contact: CreateContact) -> Result<Contact, Error> {
        let result = self.api().await?.contacts().create(contact).send().await;
        match result {
            Ok(contact) => Ok(contact),
            Err(e) if e.delivery() == Delivery::NotSent => Err(e.into()),
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
    /// Traverse search results with explicit completion, errors, and optional page cap. Add `concord-pagination` as a direct dependency and use `concord_pagination::Resume`. `Resume::Start` repeats the search body without `after`, replacing `request.after`; `Resume::At(key)` takes the `String` from `paging.next.after` in a returned page's `next`.
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

    /// Abandoning every caller while a refresh is in flight must not keep the installation (its
    /// secrets, HTTP client and cache) alive: the cache keeps the paused acquisition.
    #[tokio::test]
    async fn abandoned_refresh_does_not_keep_the_installation_alive() {
        // Accepts the connection and never answers, so the refresh stays in flight.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let hub = HubSpot::new(http, "id".into(), "secret".into(), "refresh".into())
            .unwrap()
            .with_base_url(&url)
            .unwrap();
        let installation = Arc::downgrade(&hub.inner);
        let caller = tokio::spawn({
            let hub = hub.clone();
            async move { hub.token().await.map(drop) }
        });
        let (_connection, _) = tokio::task::spawn_blocking(move || listener.accept())
            .await
            .unwrap()
            .unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        drop(hub);
        assert!(
            installation.upgrade().is_none(),
            "the abandoned refresh keeps the installation alive"
        );
    }
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
    #[test]
    fn batch_read_default_destination_and_body() {
        let request = BatchReadRequest {
            ids: vec!["1".into(), "2".into()],
            properties: vec!["email".into()],
        };
        let prepared = CrmApi::new(
            reqwest::Client::new(),
            CrmApiConfig {
                authorization: HeaderValue::from_static("Bearer t"),
            },
        )
        .contacts()
        .batch_read(request.clone())
        .prepare()
        .unwrap();
        assert_eq!(prepared.request().method(), reqwest::Method::POST);
        assert_eq!(
            prepared.request().url().as_str(),
            "https://api.hubapi.com/crm/v3/objects/contacts/batch/read"
        );
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"inputs": [{"id": "1"}, {"id": "2"}],
                "properties": ["email"], "propertiesWithHistory": []})
        );
    }
}

#[cfg(test)]
mod sdk_type_canaries {
    use super::*;

    #[test]
    fn oauth_request_withholds_credentials_in_both_debug_modes() {
        let mut canaries = concord_check_canary::Canaries::new("hubspot-request");
        let request = RefreshRequest {
            grant_type: "refresh_token",
            client_id: canaries.issue(),
            client_secret: canaries.issue(),
            refresh_token: canaries.issue(),
        };
        assert!(
            serde_json::to_string(&request)
                .unwrap()
                .contains(&request.client_secret)
        );
        canaries.assert_debug_absent("RefreshRequest", &request);
    }
}

#[cfg(test)]
mod read_operation_tests {
    use super::*;
    #[test]
    fn declared_post_reads_keep_their_method_and_effect() {
        for operation in [
            CrmApiContactsSearch::operation(),
            CrmApiContactsBatchRead::operation(),
        ] {
            assert_eq!(operation.method(), Some("POST"));
            assert!(operation.is_read(), "{}", operation.name());
        }
    }
}
