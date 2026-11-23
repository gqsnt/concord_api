//! Keep trybuild's exact comparison when rustc's rendering differs between toolchains.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

// Declare before the TestCases, which run when dropped.
pub(super) struct CompilerSnapshots {
    minor: String,
    staged: PathBuf,
}

impl CompilerSnapshots {
    pub(super) fn new() -> io::Result<Self> {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let output = Command::new(rustc).arg("--version").output()?;
        let version = String::from_utf8_lossy(&output.stdout);
        let minor = version
            .split_whitespace()
            .nth(1)
            .and_then(|release| release.split('.').nth(1))
            .filter(|minor| !minor.is_empty() && minor.bytes().all(|byte| byte.is_ascii_digit()))
            .filter(|_| output.status.success())
            .ok_or_else(|| io::Error::other(format!("cannot identify rustc: {version}")))?;
        static SUITE: AtomicUsize = AtomicUsize::new(0);
        let id = SUITE.fetch_add(1, Ordering::Relaxed);
        let staged =
            Path::new("target/diagnostic-snapshots").join(format!("{}-{id}", std::process::id()));
        Ok(Self {
            minor: format!("1.{minor}"),
            staged,
        })
    }

    pub(super) fn compile_fail(
        &self,
        tests: &trybuild::TestCases,
        directory: &str,
    ) -> io::Result<()> {
        let mut paths = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.retain(|path| path.extension().is_some_and(|ext| ext == "rs"));
        assert!(!paths.is_empty(), "no diagnostic fixtures in {directory}");
        paths.sort_unstable();
        for path in paths {
            let alternate = path.with_extension(format!("rust-{}.stderr", self.minor));
            if !alternate.try_exists()? {
                tests.compile_fail(path);
                continue;
            }

            // Trybuild reads a single sibling .stderr. Stage both files under this crate
            // so it still checks line numbers in the source. These fixtures stand alone.
            let staged = self.staged.join(&path);
            let parent = staged
                .parent()
                .ok_or_else(|| io::Error::other("staged fixture has no parent directory"))?;
            fs::create_dir_all(parent)?;
            fs::copy(&path, &staged)?;
            let snapshot = fs::read_to_string(alternate)?;
            let original = path.display().to_string().replace('\\', "/");
            let destination = staged.display().to_string().replace('\\', "/");
            fs::write(
                staged.with_extension("stderr"),
                snapshot.replace(&original, &destination),
            )?;
            tests.compile_fail(staged);
        }
        Ok(())
    }
}

impl Drop for CompilerSnapshots {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.staged);
    }
}
