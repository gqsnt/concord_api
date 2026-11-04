//! Staged shard files: each result part is written to its own temporary file and published (an
//! atomic rename) only when the part was parsed and written to the end.
//!
//! Layout inside the caller-owned staging directory: `part-000007.csv.partial` while a part is
//! being written, `part-000007.csv` once published. A [`StagedShard`] that is dropped without
//! [`publish`](StagedShard::publish) — a parse or transport error, a cancelled job future —
//! removes its partial file, so a partial part is never published as complete. Published shards
//! are never removed by this module: they are the receipts a restart reuses. The module touches
//! no other file of the directory.
//!
//! The files are written with `std::fs` (blocking writes of at most one encoded batch at a time):
//! the example has no async file dependency. A production sink would hand them to a blocking
//! pool or an async file API.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use concord_example_salesforce_sdk::bulk_query::ResultPart;

/// The staging directory of one export.
#[derive(Clone, Debug)]
pub struct ShardStaging {
    dir: PathBuf,
}

impl ShardStaging {
    /// Uses (and creates if needed) `dir`.
    ///
    /// # Errors
    ///
    /// The I/O error of creating it.
    pub fn new(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// The directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The published path of a part.
    pub fn published_path(&self, ordinal: u64) -> PathBuf {
        self.dir.join(format!("part-{ordinal:06}.csv"))
    }

    /// The staged (partial) path of a part.
    pub fn staged_path(&self, ordinal: u64) -> PathBuf {
        self.dir.join(format!("part-{ordinal:06}.csv.partial"))
    }

    /// Starts the staged file of `part` (truncating a stale partial file of the same part).
    ///
    /// # Errors
    ///
    /// The I/O error of creating it.
    pub fn begin(&self, part: &ResultPart) -> io::Result<StagedShard> {
        let staged = self.staged_path(part.ordinal);
        let file = File::create(&staged)?;
        Ok(StagedShard {
            file: Some(BufWriter::new(file)),
            staged,
            published: self.published_path(part.ordinal),
            part: part.clone(),
            bytes: 0,
        })
    }

    /// Partial files currently present (for cleanup assertions and diagnostics).
    ///
    /// # Errors
    ///
    /// The I/O error of listing the directory.
    pub fn partial_files(&self) -> io::Result<Vec<PathBuf>> {
        let mut found = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("part-") && n.ends_with(".csv.partial"))
            {
                found.push(path);
            }
        }
        found.sort();
        Ok(found)
    }
}

/// One part's file being written. Dropped unpublished, it removes itself.
pub struct StagedShard {
    file: Option<BufWriter<File>>,
    staged: PathBuf,
    published: PathBuf,
    part: ResultPart,
    bytes: u64,
}

impl StagedShard {
    /// Appends encoded rows.
    ///
    /// # Errors
    ///
    /// The I/O error.
    pub fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("shard already closed"))?;
        file.write_all(bytes)?;
        self.bytes += bytes.len() as u64;
        Ok(())
    }

    /// Flushes, syncs and renames the file to its published name.
    ///
    /// # Errors
    ///
    /// The I/O error (the partial file is then removed on drop).
    pub fn publish(mut self, rows: u64, declared_records: Option<u64>) -> io::Result<ShardReceipt> {
        let file = self
            .file
            .take()
            .ok_or_else(|| io::Error::other("shard already closed"))?;
        let file = file.into_inner().map_err(io::IntoInnerError::into_error)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&self.staged, &self.published)?;
        Ok(ShardReceipt {
            part: self.part.clone(),
            path: self.published.clone(),
            rows,
            bytes: self.bytes,
            declared_records,
        })
    }
}

impl Drop for StagedShard {
    fn drop(&mut self) {
        if self.file.take().is_some() || self.staged.exists() {
            // Unpublished: never leave a partial part behind.
            let _ = fs::remove_file(&self.staged);
        }
    }
}

impl std::fmt::Debug for StagedShard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagedShard")
            .field("ordinal", &self.part.ordinal)
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

/// A published shard: the part it holds, its file, the data rows (no header) and bytes written,
/// and the server's `Sforce-NumberOfRecords` when it sent one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardReceipt {
    /// The part.
    pub part: ResultPart,
    /// The published file.
    pub path: PathBuf,
    /// Data rows written.
    pub rows: u64,
    /// Bytes written.
    pub bytes: u64,
    /// `Sforce-NumberOfRecords`.
    pub declared_records: Option<u64>,
}

impl ShardReceipt {
    /// Whether the published file is still there with the recorded size (a receipt is reused on
    /// restart only then).
    pub fn is_intact(&self) -> bool {
        fs::metadata(&self.path).is_ok_and(|m| m.is_file() && m.len() == self.bytes)
    }
}

/// The combined export file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombinedOutput {
    /// The file.
    pub path: PathBuf,
    /// Data rows (header excluded).
    pub rows: u64,
    /// Bytes, header included.
    pub bytes: u64,
    /// Parts combined, in ordinal order.
    pub parts: u64,
}

/// Writes `header` and then every receipt's shard, in ordinal order, to `output` through a
/// partial file renamed at the end. The shards stay in place (they are the caller's to delete).
///
/// # Errors
///
/// The I/O error (the partial output is removed).
pub fn combine(
    header: &[u8],
    receipts: &[ShardReceipt],
    output: &Path,
) -> io::Result<CombinedOutput> {
    let mut ordered: Vec<&ShardReceipt> = receipts.iter().collect();
    ordered.sort_by_key(|r| r.part.ordinal);
    let partial = output.with_extension("partial");
    let result = (|| {
        let mut out = BufWriter::new(File::create(&partial)?);
        out.write_all(header)?;
        let mut bytes = header.len() as u64;
        let mut rows = 0;
        for receipt in &ordered {
            let mut shard = File::open(&receipt.path)?;
            bytes += io::copy(&mut shard, &mut out)?;
            rows += receipt.rows;
        }
        let file = out.into_inner().map_err(io::IntoInnerError::into_error)?;
        file.sync_all()?;
        Ok::<_, io::Error>((rows, bytes))
    })();
    match result {
        Ok((rows, bytes)) => {
            fs::rename(&partial, output)?;
            Ok(CombinedOutput {
                path: output.to_path_buf(),
                rows,
                bytes,
                parts: ordered.len() as u64,
            })
        }
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}
