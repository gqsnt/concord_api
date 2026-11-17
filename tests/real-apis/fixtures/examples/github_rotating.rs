use concord::auth::{
    after_401, CredentialStore, Exchange, RenewalError, Rotating, RotationDriver, RotationError,
    Stored, TokenPair,
};
use concord::{Base, Retry, Secret};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The tokens of one user, as the file holds them.
#[derive(Serialize, Deserialize)]
enum Record {
    Ready { access: String, refresh: String, expiry: Option<SystemTime> },
    Refreshing,
}

/// One user's tokens in one file of a private directory. A save writes a new file, syncs it,
/// renames it over the old one and syncs the directory, so a record is replaced whole.
#[derive(Clone)]
pub struct FileStore(pub PathBuf);

impl CredentialStore for FileStore {
    async fn load(&self) -> Result<Stored, RenewalError> {
        let bytes = tokio::fs::read(&self.0).await.map_err(RenewalError::failed)?;
        Ok(match serde_json::from_slice(&bytes).map_err(RenewalError::failed)? {
            Record::Ready { access, refresh, expiry } => {
                Stored::Ready(TokenPair::new(access, refresh, expiry))
            }
            Record::Refreshing => Stored::Refreshing,
        })
    }

    async fn save(&self, record: Stored) -> Result<(), RenewalError> {
        let record = match record {
            Stored::Ready(pair) => Record::Ready {
                access: pair.access().expose().to_string(),
                refresh: pair.refresh().expose().to_string(),
                expiry: pair.expires_at(),
            },
            Stored::Refreshing => Record::Refreshing,
        };
        let bytes = serde_json::to_vec(&record).map_err(RenewalError::failed)?;
        let pending = self.0.with_extension("pending");
        tokio::fs::write(&pending, &bytes).await.map_err(RenewalError::failed)?;
        let file = tokio::fs::File::open(&pending).await.map_err(RenewalError::failed)?;
        file.sync_all().await.map_err(RenewalError::failed)?;
        tokio::fs::rename(&pending, &self.0).await.map_err(RenewalError::failed)?;
        let directory = self.0.parent().unwrap_or(std::path::Path::new("."));
        let directory = tokio::fs::File::open(directory).await.map_err(RenewalError::failed)?;
        directory.sync_all().await.map_err(RenewalError::failed)?;
        Ok(())
    }
}

/// The client acting as one user of the app, and the driver that renews the user's token.
/// `login` is a `GithubLogin` with a timeout and no retry, in Concord and in its transport.
pub fn as_user(
    github: Github,
    login: GithubLogin,
    store: FileStore,
    client_id: String,
    client_secret: Secret,
) -> Result<(Github, RotationDriver), BoxError> {
    let base: Base = "https://api.github.com".parse()?;
    let (auth, driver) = Rotating::new(&base, store, move |exchange: Exchange| {
        let (login, id, secret) = (login.clone(), client_id.clone(), client_secret.clone());
        async move {
            match login.refresh(id, secret, exchange.refresh()).await? {
                TokenGrant::Granted { access_token, refresh_token, expires_in } => {
                    let lifetime = Duration::from_secs(expires_in);
                    Ok(TokenPair::expires_in(access_token, refresh_token, lifetime))
                }
                TokenGrant::Refused { error } => {
                    Err(RenewalError::failed(std::io::Error::other(error)))
                }
            }
        }
    });
    let github = github
        .with_middleware(auth.early_by(Duration::from_secs(60)))
        .with_retry(Retry::standard().when(after_401));
    Ok((github, driver))
}

/// Work done as the user; then the driver ends, and a pair that it could not save is saved.
pub async fn work_as_user(github: Github, driver: RotationDriver, store: FileStore)
    -> Result<(), BoxError>
{
    let driver = tokio::spawn(driver);
    let me = github.user().get().await;
    drop(github);
    match driver.await? {
        Err(RotationError::Unsaved { pair, .. }) => store.save(Stored::Ready(pair)).await?,
        ended => ended?,
    }
    let _user = me?;
    Ok(())
}
