//! Supply the facade and its dependency search paths to standalone compiler probes.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub(crate) struct Facade {
    library: PathBuf,
    dependencies: BTreeSet<PathBuf>,
}

impl Facade {
    pub(crate) fn get() -> &'static Self {
        static FACADE: OnceLock<Facade> = OnceLock::new();
        FACADE.get_or_init(Self::build)
    }

    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    fn build() -> Self {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        // The outer Cargo may still hold its build lock. Use a separate target directory.
        let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "build",
                "--offline",
                "--locked",
                "--lib",
                "-p",
                "concord-api",
                "--features",
                "multipart",
                "--message-format=json",
                "--target-dir",
            ])
            .arg(workspace.join("target/compiler-probes"))
            .current_dir(workspace)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );

        let mut library = None;
        let mut dependencies = BTreeSet::new();
        for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
            let event: artifact_json::Value = artifact_json::from_str(line).unwrap();
            if event["reason"] != "compiler-artifact" {
                continue;
            }
            for filename in event["filenames"].as_array().unwrap() {
                let path = PathBuf::from(filename.as_str().unwrap());
                dependencies.insert(path.parent().unwrap().to_owned());
                if event["target"]["name"] == "concord"
                    && path.extension().is_some_and(|ext| ext == "rlib")
                {
                    assert!(library.replace(path).is_none(), "multiple facade libraries");
                }
            }
        }
        Self {
            library: library.expect("Cargo did not report a concord facade library"),
            dependencies,
        }
    }

    pub(crate) fn link(&self, command: &mut Command) {
        for directory in &self.dependencies {
            command
                .arg("-L")
                .arg(format!("dependency={}", directory.display()));
        }
        command
            .arg("--extern")
            .arg(format!("concord={}", self.library.display()));
    }
}
