use std::fs;
use std::path::Path;

use concord_core::contracts::diff_contract_tool_snapshots;

pub(crate) fn run(before: &Path, after: &Path) -> Result<(), String> {
    let before = fs::read_to_string(before)
        .map_err(|error| format!("could not read {}: {error}", before.display()))?;
    let after = fs::read_to_string(after)
        .map_err(|error| format!("could not read {}: {error}", after.display()))?;
    let changes = diff_contract_tool_snapshots(&before, &after);
    if changes.is_empty() {
        println!("no semantic contract change");
    } else {
        for change in changes {
            println!("{}", change.label());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_fixtures_cover_every_required_classification() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let before = fs::read_to_string(root.join("fixtures/semantic_diff/before.snapshot"))
            .expect("before fixture");
        let after = fs::read_to_string(root.join("fixtures/semantic_diff/after.snapshot"))
            .expect("after fixture");
        let actual = diff_contract_tool_snapshots(&before, &after)
            .into_iter()
            .map(|change| change.label())
            .collect::<Vec<_>>()
            .join("\n");
        let expected = fs::read_to_string(root.join("fixtures/semantic_diff/expected.txt"))
            .expect("expected fixture");
        assert_eq!(format!("{actual}\n"), expected);
    }
}
