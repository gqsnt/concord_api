//! Command selection for the maintained runtime-overhead probe.

/// Stable selector for the isolated managed-retry category.
pub const MANAGED_RETRY_SELECTOR: &str = "managed-retry";

/// Printed category name for one Concord-managed rejected-status retry.
pub const MANAGED_RETRY_CATEGORY: &str = "managed retry: one rejected-status retry";

/// Benchmark category selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CategorySelection {
    /// Runs every maintained category.
    All,
    /// Runs only the isolated managed-retry category.
    ManagedRetry,
}

/// Parsed benchmark execution settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunOptions {
    /// Selected benchmark category set.
    pub selection: CategorySelection,
    /// Number of logical operations per selected category.
    pub iterations: usize,
}

/// Top-level command requested by benchmark arguments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenchmarkCommand {
    /// Executes the selected benchmark categories.
    Run(RunOptions),
    /// Prints command help.
    Help,
    /// Lists selectable category sets.
    List,
}

/// Parses benchmark arguments while retaining the existing `--smoke` behavior.
pub fn parse_arguments(
    arguments: &[String],
    configured_iterations: usize,
) -> Result<BenchmarkCommand, String> {
    let arguments = arguments
        .iter()
        .filter(|argument| argument.as_str() != "--bench")
        .collect::<Vec<_>>();
    if arguments.as_slice() == ["--help"] || arguments.as_slice() == ["-h"] {
        return Ok(BenchmarkCommand::Help);
    }
    if arguments.as_slice() == ["--list"] {
        return Ok(BenchmarkCommand::List);
    }

    let mut smoke = false;
    let mut selection = CategorySelection::All;
    let mut category_seen = false;
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument.as_str() == "--smoke" {
            if smoke {
                return Err("duplicate --smoke argument".to_owned());
            }
            smoke = true;
            index += 1;
            continue;
        }
        let category = if argument.as_str() == "--category" {
            index += 1;
            arguments
                .get(index)
                .ok_or_else(|| "--category requires a value".to_owned())?
                .as_str()
        } else if let Some(category) = argument.strip_prefix("--category=") {
            if category.is_empty() {
                return Err("--category requires a value".to_owned());
            }
            category
        } else {
            return Err(format!(
                "unknown benchmark argument `{argument}`; use --help"
            ));
        };

        if category_seen {
            return Err("duplicate --category argument".to_owned());
        }
        category_seen = true;
        selection = match category {
            "all" => CategorySelection::All,
            MANAGED_RETRY_SELECTOR => CategorySelection::ManagedRetry,
            _ => {
                return Err(format!(
                    "unknown benchmark category `{category}`; expected `all` or \
                     `{MANAGED_RETRY_SELECTOR}`"
                ));
            }
        };
        index += 1;
    }

    Ok(BenchmarkCommand::Run(RunOptions {
        selection,
        iterations: if smoke {
            1
        } else {
            configured_iterations.clamp(1, 1_000)
        },
    }))
}

/// Returns the benchmark command help.
#[must_use]
pub const fn help_text() -> &'static str {
    "Concord runtime overhead probe\n\
     \n\
     Usage:\n\
       concord_overhead [--smoke] [--category all|managed-retry]\n\
       concord_overhead --list\n\
       concord_overhead --help\n\
     \n\
     --smoke                    Run one logical operation per selected category.\n\
     --category all             Run every maintained category (default).\n\
     --category managed-retry   Run only one managed 503-to-success retry category.\n"
}

/// Returns selectable benchmark category sets.
#[must_use]
pub const fn list_text() -> &'static str {
    "all\tall maintained benchmark categories\n\
     managed-retry\tmanaged retry: one rejected-status retry\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn default_and_smoke_preserve_the_existing_iteration_contract() {
        assert_eq!(
            parse_arguments(&arguments(&["--bench"]), 37),
            Ok(BenchmarkCommand::Run(RunOptions {
                selection: CategorySelection::All,
                iterations: 37,
            }))
        );
        assert_eq!(
            parse_arguments(&arguments(&["--smoke"]), 37),
            Ok(BenchmarkCommand::Run(RunOptions {
                selection: CategorySelection::All,
                iterations: 1,
            }))
        );
    }

    #[test]
    fn managed_retry_category_is_independently_selectable() {
        assert_eq!(
            parse_arguments(
                &arguments(&["--category", MANAGED_RETRY_SELECTOR, "--smoke"]),
                37,
            ),
            Ok(BenchmarkCommand::Run(RunOptions {
                selection: CategorySelection::ManagedRetry,
                iterations: 1,
            }))
        );
        assert_eq!(
            parse_arguments(&arguments(&["--category=managed-retry"]), 4),
            Ok(BenchmarkCommand::Run(RunOptions {
                selection: CategorySelection::ManagedRetry,
                iterations: 4,
            }))
        );
    }

    #[test]
    fn help_list_and_invalid_category_are_explicit() {
        assert_eq!(
            parse_arguments(&arguments(&["--help", "--bench"]), 100),
            Ok(BenchmarkCommand::Help)
        );
        assert_eq!(
            parse_arguments(&arguments(&["--list", "--bench"]), 100),
            Ok(BenchmarkCommand::List)
        );
        assert!(help_text().contains("--category managed-retry"));
        assert!(list_text().contains(MANAGED_RETRY_CATEGORY));
        assert!(
            parse_arguments(&arguments(&["--category", "unknown"]), 100)
                .expect_err("invalid category")
                .contains("unknown benchmark category")
        );
        assert!(
            parse_arguments(&arguments(&["--category"]), 100)
                .expect_err("missing category")
                .contains("requires a value")
        );
    }
}
