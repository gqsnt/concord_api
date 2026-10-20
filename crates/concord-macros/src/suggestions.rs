//! Shared spelling selection for declaration diagnostics. Candidates contain
//! declaration names and grammar words only, never runtime values.

use std::collections::BTreeSet;

/// Returns the closest valid spellings, in a stable order.
pub(crate) fn best_matches(
    written: &str,
    candidates: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<String> {
    let normalized = normalize(written);
    let limit = (written.chars().count() / 3).max(1);
    let mut best = None;
    let mut matches = BTreeSet::new();
    for candidate in candidates {
        let candidate = candidate.as_ref();
        let rank = if normalize(candidate) == normalized {
            (0, 0)
        } else {
            let distance = edit_distance(written, candidate);
            if distance > limit {
                continue;
            }
            (1, distance)
        };
        if best.is_none_or(|best| rank < best) {
            best = Some(rank);
            matches.clear();
        }
        if best == Some(rank) {
            matches.insert(candidate.to_owned());
        }
    }
    matches.into_iter().collect()
}

/// Formats a correction suffix, or an empty string when nothing is close.
pub(crate) fn suggestion(
    written: &str,
    candidates: impl IntoIterator<Item = impl AsRef<str>>,
) -> String {
    format_matches(&best_matches(written, candidates))
}

/// Like `suggestion`, for declared identifiers rather than grammar words.
/// Rust keywords need their raw marker when the correction is pasted back.
pub(crate) fn name_suggestion(
    written: &str,
    candidates: impl IntoIterator<Item = impl AsRef<str>>,
) -> String {
    qualified_name_suggestion(written, candidates, "")
}

/// A name correction whose syntax requires a fixed qualifier.
pub(crate) fn qualified_name_suggestion(
    written: &str,
    candidates: impl IntoIterator<Item = impl AsRef<str>>,
    prefix: &str,
) -> String {
    let matches = best_matches(written, candidates)
        .into_iter()
        .map(|name| {
            let spelling = name
                .split("::")
                .map(identifier_spelling)
                .collect::<Vec<_>>()
                .join("::");
            format!("{prefix}{spelling}")
        })
        .collect::<Vec<_>>();
    format_matches(&matches)
}

/// Escapes names that could otherwise select a grammar branch at a use site.
pub(crate) fn identifier_spelling(name: &str) -> String {
    if crate::semantic::surface::is_rust_keyword(name)
        || matches!(
            name,
            "concat" | "each" | "vars" | "ep" | "secret" | "credential" | "credentials" | "off"
        )
    {
        format!("r#{name}")
    } else {
        name.to_owned()
    }
}

pub(crate) fn format_matches(matches: &[String]) -> String {
    if matches.is_empty() {
        String::new()
    } else {
        format!("; did you mean `{}`?", matches.join("` or `"))
    }
}

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|ch| *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Unrestricted Damerau–Levenshtein distance over characters. Keeping the
/// last matching row permits an insertion or deletion between transpositions.
fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<_> = left.chars().collect();
    let right: Vec<_> = right.chars().collect();
    let bound = left.len() + right.len();
    let mut rows = vec![vec![0; right.len() + 2]; left.len() + 2];
    rows[0][0] = bound;
    for i in 0..=left.len() {
        rows[i + 1][0] = bound;
        rows[i + 1][1] = i;
    }
    for j in 0..=right.len() {
        rows[0][j + 1] = bound;
        rows[1][j + 1] = j;
    }
    let mut last = std::collections::HashMap::new();
    for i in 1..=left.len() {
        let mut matched_column = 0;
        for j in 1..=right.len() {
            let prior_row = last.get(&right[j - 1]).copied().unwrap_or(0);
            let prior_column = matched_column;
            let cost = usize::from(left[i - 1] != right[j - 1]);
            if cost == 0 {
                matched_column = j;
            }
            rows[i + 1][j + 1] = (rows[i][j] + cost)
                .min(rows[i + 1][j] + 1)
                .min(rows[i][j + 1] + 1)
                .min(
                    rows[prior_row][prior_column]
                        + (i - prior_row - 1)
                        + 1
                        + (j - prior_column - 1),
                );
        }
        last.insert(left[i - 1], i);
    }
    rows[left.len() + 1][right.len() + 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_counts_characters_and_transpositions() {
        for (left, right, expected) in [
            ("", "abc", 3),
            ("tenat", "tenant", 1),
            ("ab", "ba", 1),
            ("ca", "abc", 2),
            ("éa", "aé", 1),
        ] {
            assert_eq!(edit_distance(left, right), expected);
            assert_eq!(edit_distance(right, left), expected);
        }
    }

    #[test]
    fn normalized_candidates_win_and_ties_are_stable() {
        assert_eq!(
            best_matches("tenantId", ["tenant_id", "tenantIds"]),
            ["tenant_id"]
        );
        assert_eq!(best_matches("TENANT_ID", ["tenant_id"]), ["tenant_id"]);
        assert_eq!(best_matches("cot", ["cut", "cat", "cat"]), ["cat", "cut"]);
        assert!(best_matches("xyz", ["tenant"]).is_empty());
        assert_eq!(best_matches("abcxxxxxx", ["abcyyyxxx"]), ["abcyyyxxx"]);
        assert!(best_matches("abcxxxxx", ["abcyyyxx"]).is_empty());
    }

    #[test]
    fn name_corrections_preserve_required_syntax() {
        assert_eq!(name_suggestion("typ", ["type"]), "; did you mean `r#type`?");
        assert_eq!(
            qualified_name_suggestion("tokn", ["token"], "secret."),
            "; did you mean `secret.token`?"
        );
    }
}
