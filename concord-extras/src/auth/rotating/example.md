
The application supplies the exchange and durable storage. This Unix example
uses a private, pre-existing directory, one writer, atomic rename and file and
directory synchronization. Its blocking filesystem work is explicitly started
by the application and joined before returning, including after an error: no
write can commit after `save` has returned. Encryption and cross-process leases
are application policy. The initial grant must already be stored.

This example also needs `json`, `form`, `serde`, `serde_json`, and Tokio's `rt`
and `sync` features. The application starts the driver task; Concord
starts none. It keeps the driver alive while a long-lived work queue is served.

```no_run
use concord::auth::{after_401, CredentialStore, Exchange, RenewalError, Rotating, RotationError, Stored, TokenPair};
use concord::{Base, Http, Json, Retry, Secret, Text};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::{Duration, SystemTime}};

#[derive(Serialize, Deserialize)]
enum Record {
    Ready { access: String, refresh: String, expiry: Option<SystemTime> },
    Refreshing,
}
#[derive(Clone)]
struct FileStore(PathBuf);
impl CredentialStore for FileStore {
    async fn load(&self) -> Result<Stored, RenewalError> {
        let path = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<Stored, RenewalError> {
            let bytes = std::fs::read(path).map_err(RenewalError::failed)?;
            let record: Record = serde_json::from_slice(&bytes).map_err(RenewalError::failed)?;
            Ok(match record {
                Record::Ready { access, refresh, expiry } => Stored::Ready(TokenPair::new(access, refresh, expiry)),
                Record::Refreshing => Stored::Refreshing,
            })
        }).await.map_err(RenewalError::failed)?
    }
    async fn save(&self, record: Stored) -> Result<(), RenewalError> {
        let path = self.0.clone();
        tokio::task::spawn_blocking(move || -> Result<(), RenewalError> {
            use std::io::Write;
            #[cfg(unix)]
            use std::os::unix::fs::OpenOptionsExt;
            let record = match record {
                Stored::Ready(pair) => Record::Ready {
                    access: pair.access().expose().to_string(),
                    refresh: pair.refresh().expose().to_string(), expiry: pair.expires_at(),
                },
                Stored::Refreshing => Record::Refreshing,
            };
            let bytes = serde_json::to_vec(&record).map_err(RenewalError::failed)?;
            let parent = path.parent().ok_or_else(|| RenewalError::failed(std::io::Error::other("parent required")))?;
            let temporary = path.with_extension("pending");
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(&temporary).map_err(RenewalError::failed)?;
            file.write_all(&bytes).map_err(RenewalError::failed)?;
            file.sync_all().map_err(RenewalError::failed)?;
            std::fs::rename(&temporary, &path).map_err(RenewalError::failed)?;
            std::fs::File::open(parent).and_then(|directory| directory.sync_all()).map_err(RenewalError::failed)?;
            Ok(())
        }).await.map_err(RenewalError::failed)?
    }
}
#[derive(Deserialize)]
struct TokenReply { access_token: String, refresh_token: String, expires_in: u64 }
concord::api! {
    client Tokens(key: Secret) {
        base "https://identity.example.test";
        headers { "authorization" = key; }
        POST exchange(refresh: Secret) -> TokenReply {
            path "token";
            body concord::body::form { "grant_type" = "refresh_token"; "refresh_token" = refresh; }
            read Json;
        }
    }
}
concord::api! {
    client Documents {
        base "https://documents.example.test/v2";
        GET title() -> String { path "title"; read Text; }
    }
}
async fn serve(mut jobs: tokio::sync::mpsc::Receiver<()>, store: FileStore, application_key: Secret)
    -> Result<(), Box<dyn std::error::Error>>
{
    let tokens = Tokens::new(application_key).with_timeout(Duration::from_secs(10))
        .with_retry(Retry::none())
        .with_transport(Http::with(|builder| builder.retry(concord::reqwest::retry::never()))?);
    let base: Base = "https://documents.example.test/v2".parse()?;
    let recovery_store = store.clone();
    let (auth, driver) = Rotating::new(&base, store, move |exchange: Exchange| {
        let tokens = tokens.clone();
        async move {
            let reply = tokens.exchange(exchange.refresh()).await?;
            Ok(TokenPair::expires_in(reply.access_token, reply.refresh_token, Duration::from_secs(reply.expires_in)))
        }
    });
    // This application, not Concord, starts the task and owns its lifetime.
    let driver_task = tokio::spawn(async move {
        let result = driver.await;
        if let Err(RotationError::Unsaved { pair, .. }) = &result {
            // This store joins its writes. After a store panic, first establish
            // quiescence before using any recovery store.
            if recovery_store.save(Stored::Ready(pair.clone())).await.is_ok() {
                return Ok(());
            }
        }
        result // A failed application save still returns the pair to the owner.
    });
    let documents = Documents::new().with_middleware(auth.early_by(Duration::from_secs(20)))
        .with_retry(Retry::standard().when(after_401));
    while jobs.recv().await.is_some() {
        let result = documents.title().await;
        // Handle each result here; a caller timeout does not cancel renewal.
    }
    drop(documents);
    driver_task.await??; // Drain begun work; retain any returned Unsaved error for recovery.
    Ok(())
}
```
