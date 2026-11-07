//! The client: construction, repositories, the Bearer challenge and retry, and `GET /v2/`.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use concord::reqwest::header::{AUTHORIZATION, HeaderValue, LOCATION};
use concord::reqwest::{self, StatusCode, Url};
use concord::{Prepared, Received};
use concord_auth::{RefreshError, Token, TokenCache};

use crate::api::{Registry as RootApi, Repository as RepoApi, TokenService as TokenApi};
use crate::auth::{Access, Challenge, TokenAuth};
use crate::error::{
    AuthError, AuthProblem, ERROR_BODY_LIMIT, InputError, ProtocolProblem, RegistryError,
    expect_status,
};
use crate::meta::ResponseMeta;
use crate::reference::RepositoryName;

/// The most bytes of a token document the SDK reads (64 KiB).
pub const TOKEN_BODY_LIMIT: usize = 64 * 1024;

/// A token's lifetime when the token service sends no `expires_in` (the spec's default).
const DEFAULT_TOKEN_LIFETIME: Duration = Duration::from_secs(60);

/// A client for one registry.
///
/// Construct it with [`Registry::new`] over your `reqwest::Client` (timeouts, proxies, TLS and
/// redirects are the client's). Disable redirects or reject cross-origin redirects: the client
/// follows them before the SDK can validate a response, and a redirect can expose URL query
/// state to another origin. Then answer Bearer challenges with
/// [`token_auth`](Self::token_auth). Cloning is cheap; clones share the token cache.
///
/// ```
/// use concord_sdk_distribution_registry::{Registry, RepositoryName, TokenAuth};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let http = concord::reqwest::Client::builder()
///     .redirect(concord::reqwest::redirect::Policy::none())
///     .build()?;
/// let registry = Registry::new(http, "https://registry.example.test")?
///     .token_auth(TokenAuth::anonymous().trust_realm("https://auth.example.test")?);
/// let repository = registry.repository(&RepositoryName::new("acme/app")?)?;
/// assert_eq!(repository.name().as_str(), "acme/app");
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Registry {
    http: reqwest::Client,
    base: Arc<Url>,
    root: RootApi,
    auth: Option<Arc<TokenAuth>>,
    // This instance fixes registry destination and principal/auth policy. Access supplies the
    // remaining scope; a differently configured client never shares this cache.
    tokens: Arc<TokenCache<Access, Box<AuthError>>>,
}

impl Registry {
    /// A client for the registry at `base_url` (`https://registry.example.com`, or with a path
    /// prefix the registry is served under). Without [`token_auth`](Self::token_auth), a `401`
    /// is returned as the registry's answer, its challenge in
    /// [`ApiError::challenge`](crate::ApiError).
    ///
    /// # Errors
    ///
    /// [`InputError::RegistryUrl`] unless `base_url` is an `http`/`https` URL with a host and
    /// without user information, query or fragment.
    pub fn new(http: reqwest::Client, base_url: &str) -> Result<Self, RegistryError> {
        let mut base = Url::parse(base_url).map_err(|_| InputError::RegistryUrl)?;
        if !matches!(base.scheme(), "http" | "https")
            || base.host().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(InputError::RegistryUrl.into());
        }
        let path = base.path().trim_end_matches('/').to_owned();
        base.set_path(&path);
        let root = RootApi::new(http.clone()).with_base_url(base.as_str())?;
        Ok(Self {
            http,
            base: Arc::new(base),
            root,
            auth: None,
            tokens: new_token_cache(),
        })
    }

    /// Answers Bearer challenges with `auth` (see the [`auth`](crate::auth) module). Starts a new
    /// token cache.
    pub fn token_auth(mut self, auth: TokenAuth) -> Self {
        self.auth = Some(Arc::new(auth));
        self.tokens = new_token_cache();
        self
    }

    /// The registry's base URL.
    pub fn base_url(&self) -> &Url {
        &self.base
    }

    /// One repository. Sends nothing.
    ///
    /// # Errors
    ///
    /// Concord's `Build` error if the repository's base URL cannot be formed (not expected for a
    /// validated name).
    pub fn repository(&self, name: &RepositoryName) -> Result<Repository, RegistryError> {
        let base = format!("{}/v2/{name}", self.base.as_str().trim_end_matches('/'));
        let api = RepoApi::new(self.http.clone()).with_base_url(base)?;
        Ok(Repository {
            registry: self.clone(),
            name: name.clone(),
            api,
        })
    }

    /// `GET /v2/`: whether the registry implements the V2 API. A `404` (or another non-2xx) is
    /// [`RegistryError::Api`]: the spec says to assume V2 is not implemented.
    ///
    /// # Errors
    ///
    /// A [`RegistryError`].
    pub async fn ping(&self) -> Result<ApiVersion, RegistryError> {
        let root = self.root.clone();
        let received = self
            .execute(&Access::None, |auth| {
                root.clone().maybe_authorization(auth).ping().prepare()
            })
            .await?;
        let (received, meta) = expect_status(received, StatusCode::OK).await?;
        drain(received).await?;
        Ok(ApiVersion { meta })
    }

    pub(crate) fn root(&self) -> &RootApi {
        &self.root
    }

    /// Sends the request `build` makes, answering one Bearer challenge: with the cached token
    /// for `access` (if any), then — on a `401` with a Bearer challenge, token auth configured —
    /// once more with a fresh token. Any other answer, and the second one, is returned as is.
    pub(crate) async fn execute<T, F>(
        &self,
        access: &Access,
        build: F,
    ) -> Result<Received<T>, RegistryError>
    where
        F: Fn(Option<HeaderValue>) -> Result<Prepared<T>, concord::Error> + Send,
        T: Send,
    {
        let used = self.tokens.get(access);
        let authorization = used.as_ref().map(|token| token.header().clone());
        let (client, request, decoder) = build(authorization)?.into_parts();
        // The SDK permits one challenge replay for these operations. Byte recreation is a
        // separate requirement: never poll or buffer a one-shot body just to resend it.
        let replay = request.try_clone();
        let received = decoder.execute(&client, request).await?;
        if received.status() != StatusCode::UNAUTHORIZED {
            return Ok(received);
        }
        if let Some(token) = used {
            self.tokens.invalidate(access, &token.generation());
        }
        let Some(auth) = self.auth.clone() else {
            return Ok(received);
        };
        let Some(mut replay) = replay else {
            return Ok(received);
        };
        let challenge = match Challenge::bearer(received.head().headers()) {
            Ok(Some(challenge)) => challenge,
            Ok(None) => return Ok(received),
            Err(problem) => {
                let meta = ResponseMeta::capture(&received);
                return Err(AuthError::new(problem).with_meta(meta).into());
            }
        };
        let meta = ResponseMeta::capture(&received);
        // Validate every challenge, including a stale 401 received after a newer token was
        // cached. Reusing that token never grants permission to trust a different realm.
        let realm = auth
            .check_realm(&challenge.realm, &self.base)
            .map_err(|problem| AuthError::new(problem).with_meta(meta.clone()))?;
        // The 401 body is read bounded and dropped; token calls are separate from the two
        // application attempts. A token acquisition contains no application retry loop.
        let _ = received.bytes_limited(ERROR_BODY_LIMIT).await;
        let http = self.http.clone();
        let acquisition_access = access.clone();
        let token = self
            .tokens
            .get_or_refresh(access.clone(), move || {
                Self::fetch_token(http, realm, auth, challenge, acquisition_access)
            })
            .await
            .map_err(|error| {
                let mut error = match error {
                    RefreshError::Acquire(error) => (**error).clone(),
                    RefreshError::LifetimeOverflow => AuthError::new(AuthProblem::TokenResponse),
                    RefreshError::Capacity => AuthError::new(AuthProblem::TokenTransport),
                };
                if error.meta.is_none() {
                    error.meta = Some(meta);
                }
                error
            })?;
        replay
            .headers_mut()
            .insert(AUTHORIZATION, token.header().clone());
        let received = decoder.execute(&client, replay).await?;
        if received.status() == StatusCode::UNAUTHORIZED {
            self.tokens.invalidate(access, &token.generation());
        }
        Ok(received)
    }

    /// Asks the already trusted realm for one token. This future deliberately owns only the
    /// inputs it needs, never the Registry/cache: an in-flight entry cannot retain itself.
    async fn fetch_token(
        http: reqwest::Client,
        realm: Url,
        auth: Arc<TokenAuth>,
        challenge: Challenge,
        access: Access,
    ) -> Result<Token, Box<AuthError>> {
        let scopes: Vec<String> = if challenge.scopes.is_empty() {
            access.scope().into_iter().collect()
        } else {
            challenge.scopes.clone()
        };
        let api = TokenApi::new(http)
            .with_base_url(realm.as_str())
            .map_err(|error| AuthError::new(AuthProblem::InvalidRealm).with_source(error))?
            .maybe_credentials(auth.credentials());
        let request = api
            .token(scopes)
            .maybe_service(challenge.service.clone())
            .maybe_client_id(auth.client_id_value())
            .prepare()
            .map_err(|error| AuthError::new(AuthProblem::TokenTransport).with_source(error))?;
        let sent = Instant::now();
        let received = request
            .receive()
            .await
            .map_err(|error| AuthError::new(AuthProblem::TokenTransport).with_source(error))?;
        if !received.status().is_success() {
            let meta = ResponseMeta::capture(&received);
            let status = received.status();
            let _ = received.bytes_limited(ERROR_BODY_LIMIT).await;
            let mut error = AuthError::new(AuthProblem::TokenRefused).with_meta(meta);
            error.status = Some(status);
            return Err(Box::new(error));
        }
        let (body, meta) = received
            .decode_with_meta()
            .await
            .map_err(|error| AuthError::new(AuthProblem::TokenResponse).with_source(error))?;
        let meta = ResponseMeta::from_concord(meta);
        let token = body
            .token
            .filter(|token| !token.is_empty())
            .or(body.access_token.filter(|token| !token.is_empty()))
            .ok_or_else(|| AuthError::new(AuthProblem::TokenResponse).with_meta(meta.clone()))?;
        let header = concord_auth::bearer(&token)
            .map_err(|_| AuthError::new(AuthProblem::TokenResponse).with_meta(meta.clone()))?;
        let lifetime = body
            .expires_in
            .map_or(DEFAULT_TOKEN_LIFETIME, Duration::from_secs);
        // Preserve the token response's metadata on the existing M0 overflow refusal. The
        // generic cache independently checks its own acquisition-start clock plus lifetime.
        sent.checked_add(lifetime)
            .ok_or_else(|| AuthError::new(AuthProblem::TokenResponse).with_meta(meta.clone()))?;
        Ok(Token::new(header, lifetime))
    }

    /// The single `Location` of a response, resolved against the URL that was requested, and
    /// kept on the registry's origin.
    pub(crate) fn location(
        &self,
        received_url: &Url,
        meta: &ResponseMeta,
    ) -> Result<Option<Url>, RegistryError> {
        let protocol = |problem| RegistryError::protocol(problem, Some(meta.clone()));
        let Some(value) = concord::field::single(meta.headers(), LOCATION)
            .map_err(|_| protocol(ProtocolProblem::MalformedLocation))?
        else {
            return Ok(None);
        };
        let text = value
            .to_str()
            .map_err(|_| protocol(ProtocolProblem::MalformedLocation))?;
        let url = received_url
            .join(text.trim())
            .map_err(|_| protocol(ProtocolProblem::MalformedLocation))?;
        if url.origin() != self.base.origin()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(protocol(ProtocolProblem::ForeignLocation));
        }
        if url.fragment().is_some() || url.cannot_be_a_base() {
            return Err(protocol(ProtocolProblem::MalformedLocation));
        }
        Ok(Some(url))
    }
}

fn new_token_cache() -> Arc<TokenCache<Access, Box<AuthError>>> {
    Arc::new(TokenCache::new(
        NonZeroUsize::new(1024).expect("nonzero cache capacity"),
        Duration::ZERO,
    ))
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("base", &self.base.as_str())
            .field("auth", &self.auth)
            .finish_non_exhaustive()
    }
}

/// The answer of `GET /v2/`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ApiVersion {
    /// The response metadata.
    pub meta: ResponseMeta,
}

impl ApiVersion {
    /// `Docker-Distribution-API-Version`; the spec says it should be `registry/2.0` (a client
    /// may fall back to an older API when it is absent).
    pub fn version(&self) -> Option<&str> {
        self.meta.api_version()
    }
}

/// One repository of a [`Registry`]: tags, manifests, blobs and uploads under `/v2/<name>/`.
/// Cloning is cheap.
#[derive(Clone)]
pub struct Repository {
    pub(crate) registry: Registry,
    name: RepositoryName,
    pub(crate) api: RepoApi,
}

impl Repository {
    /// The repository's name.
    pub fn name(&self) -> &RepositoryName {
        &self.name
    }

    /// The registry.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub(crate) fn pull(&self) -> Access {
        Access::Pull(self.name.clone())
    }

    pub(crate) fn push(&self) -> Access {
        Access::Push(self.name.clone())
    }
}

impl fmt::Debug for Repository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Repository")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Reads and drops a body the protocol documents as empty (bounded).
pub(crate) async fn drain<T>(received: Received<T>) -> Result<(), RegistryError> {
    received.bytes_limited(ERROR_BODY_LIMIT).await?;
    Ok(())
}

/// The same request with its last path segment written verbatim: a digest's `:` (and `+`) are
/// path characters the spec's URLs show unescaped, but Concord escapes every non-unreserved
/// character of a path value (`sha256%3A...`).
pub(crate) fn verbatim_last_segment<T>(
    prepared: Prepared<T>,
    segment: &str,
) -> Result<Prepared<T>, concord::Error> {
    let mut url = prepared.request().url().clone();
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.pop().push(segment);
    }
    prepared.follow(url.as_str())
}

/// The error a response that should have carried a `Location` gets without one.
pub(crate) fn missing_location(meta: &ResponseMeta) -> RegistryError {
    RegistryError::protocol(ProtocolProblem::MissingLocation, Some(meta.clone()))
}
