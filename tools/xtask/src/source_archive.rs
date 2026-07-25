use std::fs;
use std::path::Path;
use std::process::Command;

const OUTPUT: &str = "target/concord-source.zip";

pub(crate) fn run(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root.join("target"))
        .map_err(|error| format!("could not create the target directory: {error}"))?;
    let status = Command::new("git")
        .current_dir(root)
        .args([
            "archive",
            "--format=zip",
            "--output=target/concord-source.zip",
            "HEAD",
        ])
        .status()
        .map_err(|error| format!("could not run git archive: {error}"))?;
    if !status.success() {
        return Err(format!("git archive exited with {status}"));
    }
    println!("{}", root.join(OUTPUT).display());
    Ok(())
}
