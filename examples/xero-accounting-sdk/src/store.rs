//! Durable, fail-closed token rotation journal for a private local filesystem directory.
use crate::{Error, ErrorKind, TenantId};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const STORE_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Pair {
    pub access: String,
    pub refresh: String,
    pub expires_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
enum State {
    Ready(Pair),
    Refreshing,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    tenant: TenantId,
    client_id: String,
    scopes: Vec<String>,
    state: State,
}

/// An already-authorized authorization-code grant. This SDK does not run browser consent.
/// The application must obtain the tenant from its authorized connections, not from token claims.
pub struct Grant(Record);
impl Grant {
    /// Validate a tenant-bound credential pair and scopes, ready for durable initialization.
    /// `offline_access` and invoice read or write access are required. Expiry may be in the past.
    pub fn new(
        tenant: TenantId,
        client_id: impl Into<String>,
        access: impl Into<String>,
        refresh: impl Into<String>,
        scopes: &str,
        expires_at: SystemTime,
    ) -> Result<Self, Error> {
        let access = access.into();
        let refresh = refresh.into();
        let client_id = client_id.into();
        validate_tokens(&access, &refresh)?;
        if client_id.is_empty() || client_id.len() > 256 || client_id.chars().any(char::is_control)
        {
            return Err(Error::input("invalid OAuth client ID"));
        }
        let mut scopes: Vec<_> = scopes.split_ascii_whitespace().map(str::to_owned).collect();
        scopes.sort();
        scopes.dedup();
        if !scope(&scopes, "offline_access") || !can_read(&scopes) {
            return Err(Error::input(
                "offline_access and invoice access are required",
            ));
        }
        let expires_at = expires_at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::input("invalid expiry"))?
            .as_secs();
        Ok(Self(Record {
            tenant,
            client_id,
            scopes,
            state: State::Ready(Pair {
                access,
                refresh,
                expires_at,
            }),
        }))
    }
}
impl fmt::Debug for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Grant(<redacted>)")
    }
}
pub(crate) fn scope(scopes: &[String], value: &str) -> bool {
    scopes.iter().any(|s| s == value)
}
pub(crate) fn can_write(scopes: &[String]) -> bool {
    scope(scopes, "accounting.invoices") || scope(scopes, "accounting.transactions")
}
fn can_read(scopes: &[String]) -> bool {
    can_write(scopes)
        || scope(scopes, "accounting.invoices.read")
        || scope(scopes, "accounting.transactions.read")
}
pub(crate) fn validate_tokens(access: &str, refresh: &str) -> Result<(), Error> {
    if access.len() > 16 * 1024
        || refresh.len() > 16 * 1024
        || concord_auth::bearer(access).is_err()
        || concord_auth::bearer(refresh).is_err()
    {
        return Err(Error::input("invalid credential syntax"));
    }
    Ok(())
}
pub(crate) fn epoch() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| storage())
}
fn storage() -> Error {
    Error::new(ErrorKind::Storage)
}
fn recovery() -> Error {
    Error::new(ErrorKind::RecoveryRequired)
}

/// One tenant and OAuth principal's durable token journal.
///
/// Uses exclusive create-new locking, fsync, and same-directory atomic rename. A crash leaves
/// either a complete record or a recoverable pending pair. A stale lock or interrupted remote
/// exchange fails closed. See the README for the unavoidable remote/local crash boundary.
/// The directory must be application-private, on a local filesystem honoring rename/fsync.
#[derive(Clone)]
pub struct FileStore {
    directory: PathBuf,
    tenant: TenantId,
    client_id: String,
}
impl fmt::Debug for FileStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FileStore(<redacted>)")
    }
}
impl FileStore {
    /// Create a new private directory and durably save the initial pair together.
    /// Existing directories are refused; an existing grant is never overwritten.
    pub fn create(directory: impl AsRef<Path>, grant: Grant) -> Result<Self, Error> {
        let directory = directory.as_ref().to_path_buf();
        #[cfg_attr(not(unix), allow(unused_mut))] // the private mode is set on Unix only
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(|_| storage())?;
        let store = Self {
            directory,
            tenant: grant.0.tenant.clone(),
            client_id: grant.0.client_id.clone(),
        };
        store.write_new("pending.json", &grant.0)?;
        store.publish()?;
        Ok(store)
    }
    /// Reopen a journal without changing its tenant or OAuth principal.
    pub fn open(
        directory: impl AsRef<Path>,
        tenant: TenantId,
        client_id: impl Into<String>,
    ) -> Result<Self, Error> {
        let store = Self {
            directory: directory.as_ref().to_path_buf(),
            tenant,
            client_id: client_id.into(),
        };
        let _lock = store.lock()?;
        let record = store.read("state.json")?;
        store.check(&record)?;
        Ok(store)
    }
    /// Tenant permanently associated with this journal.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }
    pub(crate) fn client_id(&self) -> &str {
        &self.client_id
    }
    fn check(&self, record: &Record) -> Result<(), Error> {
        if record.tenant != self.tenant || record.client_id != self.client_id {
            return Err(Error::new(ErrorKind::TenantMismatch));
        }
        Ok(())
    }
    fn read(&self, name: &str) -> Result<Record, Error> {
        let mut data = Vec::new();
        File::open(self.directory.join(name))
            .map_err(|_| storage())?
            .take(STORE_LIMIT + 1)
            .read_to_end(&mut data)
            .map_err(|_| storage())?;
        if data.len() as u64 > STORE_LIMIT {
            return Err(storage());
        }
        serde_json::from_slice(&data).map_err(|_| storage())
    }
    fn write_new(&self, name: &str, record: &Record) -> Result<(), Error> {
        let bytes = serde_json::to_vec(record).map_err(|_| storage())?;
        if bytes.len() as u64 > STORE_LIMIT {
            return Err(storage());
        }
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts
            .open(self.directory.join(name))
            .map_err(|_| storage())?;
        file.write_all(&bytes).map_err(|_| storage())?;
        file.sync_all().map_err(|_| storage())?;
        self.sync_directory()
    }
    fn sync_directory(&self) -> Result<(), Error> {
        File::open(&self.directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| storage())
    }
    fn publish(&self) -> Result<(), Error> {
        fs::rename(
            self.directory.join("pending.json"),
            self.directory.join("state.json"),
        )
        .map_err(|_| storage())?;
        self.sync_directory()
    }
    fn lock(&self) -> Result<Lock, Error> {
        let path = self.directory.join("lock");
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(&path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::new(ErrorKind::StoreBusy)
                } else {
                    storage()
                }
            })?
            .sync_all()
            .map_err(|_| storage())?;
        self.sync_directory()?;
        Ok(Lock { path })
    }
    pub(crate) fn info(&self) -> Result<(Vec<String>, Pair), Error> {
        let _lock = self.lock()?;
        if self.directory.join("pending.json").exists() {
            return Err(recovery());
        }
        let record = self.read("state.json")?;
        self.check(&record)?;
        match record.state {
            State::Ready(pair) => Ok((record.scopes, pair)),
            State::Refreshing => Err(recovery()),
        }
    }
    pub(crate) fn begin(&self, force: bool) -> Result<Acquisition, Error> {
        let lock = self.lock()?;
        if self.directory.join("pending.json").exists() {
            return Err(recovery());
        }
        let mut record = self.read("state.json")?;
        self.check(&record)?;
        let State::Ready(pair) = &record.state else {
            return Err(recovery());
        };
        let pair = pair.clone();
        if !force && pair.expires_at > epoch()?.saturating_add(30) {
            return Ok(Acquisition::Ready(pair));
        }
        // This durable marker removes the old refresh token from the active record BEFORE I/O.
        // Any lost response or cancellation can therefore never silently replay it.
        record.state = State::Refreshing;
        self.write_new("pending.json", &record)?;
        self.publish()?;
        Ok(Acquisition::Refresh(Box::new(Ticket {
            store: self.clone(),
            record,
            old: pair,
            _lock: lock,
        })))
    }
    /// Publish a complete pending rotated pair after a failed activation, without network I/O.
    /// A partial pending file, stale process lock or lost remote response is refused. Resolve
    /// those with a fresh authorization grant in a new directory, rather than replaying old tokens.
    pub fn recover_pending(&self) -> Result<(), Error> {
        let _lock = self.lock()?;
        let record = self.read("pending.json")?;
        self.check(&record)?;
        let State::Ready(pair) = &record.state else {
            return Err(recovery());
        };
        validate_tokens(&pair.access, &pair.refresh)?;
        self.publish()
    }
    /// Remaining access lifetime from the durable record; no token strings are exposed.
    pub fn remaining_lifetime(&self) -> Result<Duration, Error> {
        let (_, pair) = self.info()?;
        Ok(Duration::from_secs(
            pair.expires_at.saturating_sub(epoch()?),
        ))
    }
}
struct Lock {
    path: PathBuf,
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
pub(crate) enum Acquisition {
    Ready(Pair),
    Refresh(Box<Ticket>),
}
pub(crate) struct Ticket {
    store: FileStore,
    record: Record,
    pub old: Pair,
    _lock: Lock,
}
impl Ticket {
    pub fn client_id(&self) -> &str {
        &self.record.client_id
    }
    pub fn scopes(&self) -> &[String] {
        &self.record.scopes
    }
    pub fn commit(mut self, pair: Pair) -> Result<Pair, Error> {
        self.record.state = State::Ready(pair.clone());
        // Full pair reaches durable pending storage before publication; publication failure
        // keeps it there for recover_pending. No access token escapes before both steps succeed.
        self.store.write_new("pending.json", &self.record)?;
        self.store.publish()?;
        Ok(pair)
    }
}
