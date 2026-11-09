//! Bounded sinks for downloaded artifacts.
//!
//! A sink receives chunks as the transfer delivers them (the SDK's
//! [`ByteSink`]); nothing collects the body
//! first. [`ArtifactSink`] adds the two decisions the workflow makes at the end: `commit` a
//! complete transfer, or `abort` a failed or cancelled one so that partial output is never taken
//! for a complete archive.
//!
//! - [`BoundedBuffer`]: in memory, refusing to grow past its limit.
//! - [`FileSink`]: writes `<file>.partial` and renames it to `<file>` on commit; abort removes
//!   the partial file. It uses blocking `std::fs` writes per chunk (chunks are tens of KiB); an
//!   application on a busy runtime would move them to a blocking pool.
//!
//! Both honour a [`CancelFlag`]: once it is raised, the next chunk is refused with
//! [`SinkError::Cancelled`], which stops the transfer (the SDK drops the response).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use concord::bytes::Bytes;
use concord_example_github_sdk::download::ByteSink;

/// A sink the artifact workflow can commit or abort.
pub trait ArtifactSink: ByteSink {
    /// The transfer completed: make the output final. Returns the bytes kept.
    ///
    /// # Errors
    ///
    /// The sink's error (a failed rename, say); the output is then not final.
    fn commit(&mut self) -> Result<u64, Self::Error>;
    /// The transfer failed or was cancelled: discard partial output.
    fn abort(&mut self);
}

/// A cancellation signal shared between a sink and whoever may cancel the transfer.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    /// A flag that is not raised.
    pub fn new() -> Self {
        Self::default()
    }

    /// Raises it: the sink refuses its next chunk.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether it was raised.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Why a sink refused a chunk or could not commit.
#[derive(Debug)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_sink_error)]
pub enum SinkError {
    /// The chunk would exceed the sink's limit.
    Full {
        /// The limit in bytes.
        limit: u64,
    },
    /// The [`CancelFlag`] was raised.
    Cancelled,
    /// A file operation failed.
    #[from(skip)]
    Io(std::io::Error),
}

fn display_sink_error(value: &SinkError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        SinkError::Full { limit } => write!(f, "sink full: limit {limit} bytes"),
        SinkError::Cancelled => f.write_str("sink cancelled"),
        SinkError::Io(error) => write!(f, "sink I/O failed: {}", error.kind()),
    }
}

/// An in-memory sink that refuses to grow past `limit` bytes.
#[derive(Debug)]
pub struct BoundedBuffer {
    limit: u64,
    data: Vec<u8>,
    cancel: CancelFlag,
    committed: bool,
}

impl BoundedBuffer {
    /// A buffer of at most `limit` bytes.
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            data: Vec::new(),
            cancel: CancelFlag::new(),
            committed: false,
        }
    }

    /// With a cancellation flag.
    pub fn with_cancel(mut self, cancel: CancelFlag) -> Self {
        self.cancel = cancel;
        self
    }

    /// The bytes received (complete only after a successful commit).
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Whether the content was committed.
    pub fn is_committed(&self) -> bool {
        self.committed
    }
}

impl ByteSink for BoundedBuffer {
    type Error = SinkError;

    async fn write(&mut self, chunk: Bytes) -> Result<(), SinkError> {
        if self.cancel.is_cancelled() {
            return Err(SinkError::Cancelled);
        }
        if self.data.len() as u64 + chunk.len() as u64 > self.limit {
            return Err(SinkError::Full { limit: self.limit });
        }
        self.data.extend_from_slice(&chunk);
        Ok(())
    }
}

impl ArtifactSink for BoundedBuffer {
    fn commit(&mut self) -> Result<u64, SinkError> {
        self.committed = true;
        Ok(self.data.len() as u64)
    }

    fn abort(&mut self) {
        self.data.clear();
        self.committed = false;
    }
}

/// A file sink: `<path>.partial` while writing, renamed to `<path>` on commit, removed on abort.
#[derive(Debug)]
pub struct FileSink {
    path: PathBuf,
    partial: PathBuf,
    file: Option<File>,
    limit: u64,
    written: u64,
    cancel: CancelFlag,
}

impl FileSink {
    /// A sink for `path`, of at most `limit` bytes. Nothing is created before the first chunk.
    pub fn new(path: impl Into<PathBuf>, limit: u64) -> Self {
        let path = path.into();
        let mut partial = path.clone().into_os_string();
        partial.push(".partial");
        Self {
            path,
            partial: PathBuf::from(partial),
            file: None,
            limit,
            written: 0,
            cancel: CancelFlag::new(),
        }
    }

    /// With a cancellation flag.
    pub fn with_cancel(mut self, cancel: CancelFlag) -> Self {
        self.cancel = cancel;
        self
    }

    /// The final path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The partial path used while writing.
    pub fn partial_path(&self) -> &Path {
        &self.partial
    }

    /// Bytes written so far.
    pub fn written(&self) -> u64 {
        self.written
    }
}

impl ByteSink for FileSink {
    type Error = SinkError;

    async fn write(&mut self, chunk: Bytes) -> Result<(), SinkError> {
        if self.cancel.is_cancelled() {
            return Err(SinkError::Cancelled);
        }
        if self.written + chunk.len() as u64 > self.limit {
            return Err(SinkError::Full { limit: self.limit });
        }
        let file = match &mut self.file {
            Some(file) => file,
            slot => slot.insert(File::create(&self.partial).map_err(SinkError::Io)?),
        };
        file.write_all(&chunk).map_err(SinkError::Io)?;
        self.written += chunk.len() as u64;
        Ok(())
    }
}

impl ArtifactSink for FileSink {
    fn commit(&mut self) -> Result<u64, SinkError> {
        let file = match self.file.take() {
            Some(file) => file,
            // An empty archive still produces a file.
            None => File::create(&self.partial).map_err(SinkError::Io)?,
        };
        file.sync_all().map_err(SinkError::Io)?;
        drop(file);
        fs::rename(&self.partial, &self.path).map_err(SinkError::Io)?;
        Ok(self.written)
    }

    fn abort(&mut self) {
        self.file = None;
        // Only the sink's own partial file is removed; a missing file is fine.
        let _ = fs::remove_file(&self.partial);
    }
}
