//! Scoped single-flight acquisition, with persistence before cache publication.
use crate::{
    Error, ErrorKind, FileStore,
    policy::finish,
    store::{Acquisition, Pair, epoch, validate_tokens},
};
use concord::{
    Endpoint, SendWith,
    reqwest::{
        self,
        header::{AUTHORIZATION, HeaderValue},
    },
};
use concord_auth::{RefreshError, Snapshot, Token, TokenCache};
use serde::Deserialize;
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
}
#[derive(Clone, Default)]
struct TokenPolicy;
impl<T: Send> SendWith<T> for TokenPolicy {
    type Output = Result<T, Error>;
    async fn send<E: Endpoint<T> + Send>(&self, op: E) -> Self::Output {
        finish(op.prepare()?.receive().await?).await
    }
}
concord::api! {
    client OAuth(#[sensitive] header(AUTHORIZATION) credential:HeaderValue) {
        base "https://identity.xero.com";
        request concord::Form;
        response concord::Json;
        limit 64*1024;
        send TokenPolicy -> Result<_,Error>;
        POST refresh(body form:Vec<(String,String)>) -> TokenResponse {path "connect/token";}
    }
}

#[derive(Clone, concord::Debug)]
#[sensitive]
pub(crate) struct Auth {
    pub store: FileStore,
    cache: Arc<TokenCache<(), Error>>,
    force: Arc<Mutex<bool>>,
    oauth: OAuth,
    scopes: Vec<String>,
}
impl Auth {
    pub fn new(
        store: FileStore,
        secret: &str,
        token_base: &str,
        http: reqwest::Client,
    ) -> Result<Self, Error> {
        let (scopes, _) = store.info()?;
        let credential = concord_auth::basic(store.client_id(), secret)
            .map_err(|_| Error::input("invalid OAuth client credentials"))?;
        let oauth = OAuth::new(http, OAuthConfig { credential }).with_base_url(token_base)?;
        Ok(Self {
            store,
            scopes,
            oauth,
            force: Arc::new(Mutex::new(false)),
            cache: Arc::new(TokenCache::new(
                NonZeroUsize::new(1).unwrap(),
                Duration::from_secs(30),
            )),
        })
    }
    pub fn require(&self, write: bool) -> Result<(), Error> {
        if write && !crate::store::can_write(&self.scopes) {
            return Err(Error::input("invoice write scope is required"));
        }
        Ok(())
    }
    pub async fn token(&self) -> Result<Snapshot, Error> {
        // Capture acquisition inputs, never this cache (which would form an Arc cycle).
        let store = self.store.clone();
        let oauth = self.oauth.clone();
        let force = self.force.clone();
        self.cache
            .get_or_refresh((), move || async move {
                let force = std::mem::take(&mut *force.lock().unwrap_or_else(|e| e.into_inner()));
                let acquisition = blocking(move || store.begin(force)).await?;
                let pair = match acquisition {
                    Acquisition::Ready(pair) => pair,
                    Acquisition::Refresh(ticket) => {
                        let ticket = *ticket;
                        let form = vec![
                            ("grant_type".into(), "refresh_token".into()),
                            ("client_id".into(), ticket.client_id().to_owned()),
                            ("refresh_token".into(), ticket.old.refresh.clone()),
                        ];
                        let response = oauth.refresh(form).send().await?;
                        validate_tokens(&response.access_token, &response.refresh_token)?;
                        if !response.token_type.eq_ignore_ascii_case("Bearer")
                            || response.expires_in <= 30
                            || response.expires_in > 1800
                        {
                            return Err(Error::new(ErrorKind::Contract(
                                "invalid refreshed token type or lifetime",
                            )));
                        }
                        if let Some(scope) = response.scope {
                            let mut returned: Vec<String> =
                                scope.split_ascii_whitespace().map(str::to_owned).collect();
                            returned.sort();
                            returned.dedup();
                            if returned != ticket.scopes() {
                                return Err(Error::new(ErrorKind::Contract(
                                    "refreshed token scopes changed",
                                )));
                            }
                        }
                        let expires_at =
                            epoch()?.checked_add(response.expires_in).ok_or_else(|| {
                                Error::new(ErrorKind::Contract("token expiry overflow"))
                            })?;
                        let pair = Pair {
                            access: response.access_token,
                            refresh: response.refresh_token,
                            expires_at,
                        };
                        blocking(move || ticket.commit(pair)).await?
                    }
                };
                let lifetime = Duration::from_secs(pair.expires_at.saturating_sub(epoch()?));
                let header = concord_auth::bearer(&pair.access)
                    .map_err(|_| Error::input("invalid access token"))?;
                Ok(Token::new(header, lifetime))
            })
            .await
            .map_err(|e| match e {
                RefreshError::Acquire(e) => Error::new(ErrorKind::Authentication(e)),
                RefreshError::Capacity | RefreshError::LifetimeOverflow => {
                    Error::new(ErrorKind::Contract("credential cache refused acquisition"))
                }
            })
    }
    pub fn invalidate(&self, used: &Snapshot) {
        // Synchronize the generation removal and refresh flag. Acquisition reads this same
        // lock, so it cannot observe a removed cache entry before the flag is set.
        let mut force = self.force.lock().unwrap_or_else(|e| e.into_inner());
        if self.cache.invalidate(&(), &used.generation()) {
            *force = true;
        }
    }
}
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| Error::new(ErrorKind::Storage))?
}
