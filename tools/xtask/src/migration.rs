use std::fs;
use std::path::{Path, PathBuf};

const RETIRED_DSL_PATTERNS: &[(&str, &str)] = &[
    ("vars {", "values {"),
    ("vars.", "client."),
    ("scope.", "the exact lexical scope name"),
    ("ep.", "endpoint."),
    ("fmt[", "concat ["),
    ("fmt [", "concat ["),
    ("host [", "subdomain ["),
    (
        "#[body]",
        "body NAME: INPUT via REPRESENTATION (manual type review required)",
    ),
    ("= default", "#[default]"),
    ("rate_limit use", "rate_limit PROFILE(...)"),
    ("rule ", "a named rate profile with explicit formals"),
];

const RETIRED_RUST_PATTERNS: &[(&str, &str)] = &[
    ("ResponseBodySpec", "ResponseRepresentation"),
    ("response_bytes", "accepted_bytes"),
    ("SafeRequestPlan", "RequestPlan"),
    ("PageLimit::Take", "PageLimit::Prefix"),
    ("PageLimit::HardCap", "PageLimit::CompleteWithin"),
];

const INPUT_TYPE_REVIEW: &str = "__ConcordV2_1BodyInputTypeRequiresReview";
const OUTPUT_TYPE_REVIEW: &str = "__ConcordV2_1OutputTypeRequiresReview";

pub(crate) fn run(root: &Path, write: bool) -> Result<(), String> {
    let mut files = Vec::new();
    for directory in ["crates", "fixtures", "tools"] {
        collect_rust_files(&root.join(directory), &mut files)?;
    }
    files.sort();

    let mut topology_changes = 0_usize;
    let mut timeout_changes = 0_usize;
    let mut signature_changes = 0_usize;
    let mut findings = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let visible_source = code_only(&source);
        let (rewritten, moved, timeouts, signatures) = rewrite_mechanical(&source)?;
        topology_changes += moved;
        timeout_changes += timeouts;
        signature_changes += signatures;
        if timeouts > 0 {
            findings.push(format!(
                "{}: client root needs an explicit positive finite v2.1 timeout; no value was guessed",
                relative(root, &path).display(),
            ));
        }
        if write && rewritten != source {
            fs::write(&path, rewritten)
                .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        }

        if audit_excluded(root, &path) {
            continue;
        }

        for (line_index, line) in visible_source.lines().enumerate() {
            for (retired, replacement) in RETIRED_RUST_PATTERNS {
                if line.contains(retired) {
                    findings.push(format!(
                        "{}:{}: `{retired}` -> {replacement}",
                        relative(root, &path).display(),
                        line_index + 1,
                    ));
                }
            }
            for marker in [INPUT_TYPE_REVIEW, OUTPUT_TYPE_REVIEW] {
                if line.contains(marker) {
                    findings.push(format!(
                        "{}:{}: replace `{marker}` with the exact v2.1 domain type",
                        relative(root, &path).display(),
                        line_index + 1,
                    ));
                }
            }
        }
        for (open, close) in contract_macro_ranges(&source)? {
            let prefix_lines = source[..=open]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            for (line_index, line) in visible_source[(open + 1)..close].lines().enumerate() {
                for (retired, replacement) in RETIRED_DSL_PATTERNS {
                    if line.contains(retired) {
                        findings.push(format!(
                            "{}:{}: `{retired}` -> {replacement}",
                            relative(root, &path).display(),
                            prefix_lines + line_index + 1,
                        ));
                    }
                }
                if line.contains(": Option<") || line.contains(": Vec<") {
                    findings.push(format!(
                        "{}:{}: review opaque payload type for explicit `?`/`each` intent",
                        relative(root, &path).display(),
                        prefix_lines + line_index + 1,
                    ));
                }
            }
        }
    }

    if write {
        println!(
            "migration: moved {topology_changes} client child region(s); reported {timeout_changes} missing finite root timeout(s); marked {signature_changes} ambiguous body/response signature(s) for review"
        );
    } else {
        println!("migration audit: {} review item(s)", findings.len());
    }
    for finding in findings.iter().take(500) {
        println!("{finding}");
    }
    if findings.len() > 500 {
        println!("... {} additional item(s) omitted", findings.len() - 500);
    }
    Ok(())
}

fn audit_excluded(root: &Path, path: &Path) -> bool {
    let path = relative(root, path).to_string_lossy().replace('\\', "/");
    path == "tools/xtask/src/migration.rs"
        || path.starts_with("crates/concord-macros/src/syntax/")
        || path.contains("/tests/ui/fail/")
}

fn collect_rust_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("directory entry failed: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_rust_files(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
    Ok(())
}

fn relative<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

fn rewrite_mechanical(source: &str) -> Result<(String, usize, usize, usize), String> {
    let mut rewritten = source.to_owned();
    let mut moved = 0;
    let mut timeouts = 0;
    let mut signatures = 0;

    let ranges = contract_macro_ranges(&rewritten)?;
    for (open, close) in ranges.into_iter().rev() {
        let replacement = rewrite_contract_vocabulary(&rewritten, open + 1, close);
        rewritten.replace_range((open + 1)..close, &replacement);
    }

    let ranges = contract_macro_ranges(&rewritten)?;
    for (open, close) in ranges.into_iter().rev() {
        let Some((_client_open, client_close)) = client_range(&rewritten, open, close) else {
            continue;
        };
        let mask = code_mask(rewritten.as_bytes());
        let has_tail_tokens = ((client_close + 1)..close)
            .any(|index| mask[index] && !rewritten.as_bytes()[index].is_ascii_whitespace());
        if has_tail_tokens {
            let tail = rewritten[(client_close + 1)..close].to_owned();
            rewritten.replace_range((client_close + 1)..close, "");
            rewritten.insert_str(client_close, &tail);
            moved += 1;
        }
    }

    let ranges = contract_macro_ranges(&rewritten)?;
    for (open, close) in ranges.into_iter().rev() {
        let Some((client_open, client_close)) = client_range(&rewritten, open, close) else {
            continue;
        };
        if !has_top_level_word(&rewritten, client_open + 1, client_close, "timeout") {
            timeouts += 1;
        }
    }

    let ranges = contract_macro_ranges(&rewritten)?;
    for (open, close) in ranges.into_iter().rev() {
        let replacements = signature_replacements(&rewritten, open + 1, close);
        signatures += replacements.len();
        for (start, end, replacement) in replacements.into_iter().rev() {
            rewritten.replace_range(start..end, &replacement);
        }
    }
    Ok((rewritten, moved, timeouts, signatures))
}

fn rewrite_contract_vocabulary(source: &str, start: usize, end: usize) -> String {
    let mut contract = source[start..end].to_owned();
    contract = rewrite_basic_vocabulary(&contract);
    contract = rewrite_scope_owners(&contract);
    contract = rewrite_auth_blocks(&contract);
    contract = rewrite_rate_profiles(&contract);
    contract = rewrite_rate_uses(&contract);
    contract = rewrite_rate_vocabulary(&contract);
    contract = rewrite_pagination_vocabulary(&contract);
    contract
}

fn rewrite_basic_vocabulary(contract: &str) -> String {
    let bytes = contract.as_bytes();
    let mask = code_mask(bytes);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if word_at(bytes, &mask, index, "vars") {
            let after = skip_space(bytes, index + 4, bytes.len());
            let replacement = if bytes.get(after) == Some(&b'.') {
                Some("client")
            } else if bytes.get(after) == Some(&b'{') {
                Some("values")
            } else {
                None
            };
            if let Some(replacement) = replacement {
                replacements.push((index, index + 4, replacement.to_owned()));
            }
            index += 4;
            continue;
        }
        if word_at(bytes, &mask, index, "ep") && bytes.get(index + 2) == Some(&b'.') {
            replacements.push((index, index + 2, "endpoint".to_owned()));
            index += 2;
            continue;
        }
        if word_at(bytes, &mask, index, "fmt")
            && bytes.get(skip_space(bytes, index + 3, bytes.len())) == Some(&b'[')
        {
            replacements.push((index, index + 3, "concat".to_owned()));
            index += 3;
            continue;
        }
        if word_at(bytes, &mask, index, "host") {
            let after = skip_space(bytes, index + 4, bytes.len());
            if bytes.get(after) == Some(&b'[')
                || word_at(bytes, &mask, after, "replace")
                || word_at(bytes, &mask, after, "off")
            {
                replacements.push((index, index + 4, "subdomain".to_owned()));
                index += 4;
                continue;
            }
        }
        if word_at(bytes, &mask, index, "secret") {
            let name_start = skip_space(bytes, index + 6, bytes.len());
            if let Some(name_end) = rust_ident_end(bytes, name_start, bytes.len()) {
                let colon = skip_space(bytes, name_end, bytes.len());
                if bytes.get(colon) == Some(&b':') {
                    if let Some(semicolon) =
                        top_level_delimiter(bytes, &mask, colon + 1, bytes.len(), b';')
                    {
                        replacements.push((colon, semicolon, String::new()));
                        index = semicolon;
                        continue;
                    }
                }
            }
        }
        index += 1;
    }
    apply_replacements(contract, replacements)
}

fn rewrite_rate_profiles(contract: &str) -> String {
    let bytes = contract.as_bytes();
    let mask = code_mask(bytes);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index + 4 <= bytes.len() {
        if !word_at(bytes, &mask, index, "rule") {
            index += 1;
            continue;
        }
        let name_start = skip_space(bytes, index + 4, bytes.len());
        let Some(name_end) = rust_ident_end(bytes, name_start, bytes.len()) else {
            index += 4;
            continue;
        };
        let open = skip_space(bytes, name_end, bytes.len());
        if bytes.get(open) != Some(&b'{') {
            index += 4;
            continue;
        }
        let Some(close) = matching_brace(bytes, &mask, open) else {
            index += 4;
            continue;
        };
        let mut formals = Vec::new();
        let mut cursor = open + 1;
        while cursor + 3 < close {
            if (word_at(bytes, &mask, cursor, "by") || word_at(bytes, &mask, cursor, "key"))
                && bytes.get(skip_space(
                    bytes,
                    cursor
                        + if word_at(bytes, &mask, cursor, "by") {
                            2
                        } else {
                            3
                        },
                    close,
                )) == Some(&b'[')
            {
                let keyword_len = if word_at(bytes, &mask, cursor, "by") {
                    2
                } else {
                    3
                };
                let dimensions_open = skip_space(bytes, cursor + keyword_len, close);
                if let Some(dimensions_close) =
                    matching_delimiter(bytes, &mask, dimensions_open, b'[', b']')
                {
                    for (dimension_start, dimension_end) in
                        split_top_level(bytes, &mask, dimensions_open + 1, dimensions_close, b',')
                    {
                        let dimension = contract[dimension_start..dimension_end].trim();
                        if !dimension.is_empty()
                            && !dimension.contains('.')
                            && !matches!(dimension, "host" | "endpoint")
                            && !formals.iter().any(|formal| formal == dimension)
                        {
                            formals.push(dimension.to_owned());
                        }
                    }
                }
                break;
            }
            cursor += 1;
        }
        let name = contract[name_start..name_end].trim();
        replacements.push((index, name_end, format!("{name}({})", formals.join(", "))));
        index = close + 1;
    }
    apply_replacements(contract, replacements)
}

fn rewrite_rate_uses(contract: &str) -> String {
    let bytes = contract.as_bytes();
    let mask = code_mask(bytes);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index + 10 <= bytes.len() {
        if !word_at(bytes, &mask, index, "rate_limit") {
            index += 1;
            continue;
        }
        let use_start = skip_space(bytes, index + 10, bytes.len());
        if !word_at(bytes, &mask, use_start, "use") {
            index += 10;
            continue;
        }
        let name_start = skip_space(bytes, use_start + 3, bytes.len());
        let Some(name_end) = rust_ident_end(bytes, name_start, bytes.len()) else {
            index += 10;
            continue;
        };
        let name = contract[name_start..name_end].trim();
        let after_name = skip_space(bytes, name_end, bytes.len());
        if bytes.get(after_name) == Some(&b';') {
            replacements.push((index, after_name + 1, format!("rate_limit {name}();")));
            index = after_name + 1;
            continue;
        }
        if bytes.get(after_name) != Some(&b'{') {
            index += 10;
            continue;
        }
        let Some(close) = matching_brace(bytes, &mask, after_name) else {
            index += 10;
            continue;
        };
        let mut bindings = Vec::new();
        let mut cost = None;
        for (statement_start, statement_end) in
            split_top_level(bytes, &mask, after_name + 1, close, b';')
        {
            let statement = contract[statement_start..statement_end].trim();
            if statement.is_empty() {
                continue;
            }
            if let Some(value) = statement.strip_prefix("cost ") {
                cost = Some(value.trim().to_owned());
            } else if let Some((formal, value)) = statement.split_once('=') {
                bindings.push(format!("{} = {}", formal.trim(), value.trim()));
            }
        }
        let replacement = if let Some(cost) = cost {
            format!(
                "rate_limit {name}({}) {{ cost {cost}; }}",
                bindings.join(", ")
            )
        } else {
            format!("rate_limit {name}({});", bindings.join(", "))
        };
        replacements.push((index, close + 1, replacement));
        index = close + 1;
    }
    apply_replacements(contract, replacements)
}

fn rewrite_scope_owners(contract: &str) -> String {
    let bytes = contract.as_bytes();
    let mask = code_mask(bytes);
    let mut scopes = Vec::new();
    let mut index = 0;
    while index + 5 <= bytes.len() {
        if !word_at(bytes, &mask, index, "scope") {
            index += 1;
            continue;
        }
        let name_start = skip_space(bytes, index + 5, bytes.len());
        let Some(name_end) = rust_ident_end(bytes, name_start, bytes.len()) else {
            index += 5;
            continue;
        };
        let after_name = skip_space(bytes, name_end, bytes.len());
        if bytes.get(after_name) != Some(&b'(') {
            index += 5;
            continue;
        }
        let Some(parameters_end) = matching_delimiter(bytes, &mask, after_name, b'(', b')') else {
            index += 5;
            continue;
        };
        let body_start = skip_space(bytes, parameters_end + 1, bytes.len());
        if bytes.get(body_start) != Some(&b'{') {
            index += 5;
            continue;
        }
        let Some(body_end) = matching_brace(bytes, &mask, body_start) else {
            index += 5;
            continue;
        };
        scopes.push((
            body_start + 1,
            body_end,
            contract[name_start..name_end].to_owned(),
        ));
        index = name_end;
    }

    let mut replacements = Vec::new();
    let mut index = 0;
    while index + 6 <= bytes.len() {
        if word_at(bytes, &mask, index, "scope") && bytes.get(index + 5) == Some(&b'.') {
            if let Some((_, _, name)) = scopes
                .iter()
                .filter(|(scope_start, scope_end, _)| *scope_start <= index && index < *scope_end)
                .min_by_key(|(scope_start, scope_end, _)| scope_end - scope_start)
            {
                replacements.push((index, index + 5, name.clone()));
            }
        }
        index += 1;
    }
    apply_replacements(contract, replacements)
}

fn rewrite_auth_blocks(contract: &str) -> String {
    let bytes = contract.as_bytes();
    let mask = code_mask(bytes);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index + 4 <= bytes.len() {
        if !word_at(bytes, &mask, index, "auth") {
            index += 1;
            continue;
        }
        let mut cursor = skip_space(bytes, index + 4, bytes.len());
        let replace = word_at(bytes, &mask, cursor, "replace");
        if replace {
            cursor = skip_space(bytes, cursor + 7, bytes.len());
        }
        if bytes.get(cursor) == Some(&b'{') || word_at(bytes, &mask, cursor, "off") {
            index += 4;
            continue;
        }
        let Some(statement_end) = top_level_delimiter(bytes, &mask, cursor, bytes.len(), b';')
        else {
            index += 4;
            continue;
        };
        let placement = contract[cursor..statement_end].trim();
        let operation = if replace { "auth replace" } else { "auth" };
        replacements.push((
            index,
            statement_end + 1,
            format!("{operation} {{ {placement}; }}"),
        ));
        index = statement_end + 1;
    }
    apply_replacements(contract, replacements)
}

fn rewrite_rate_vocabulary(contract: &str) -> String {
    let bytes = contract.as_bytes();
    let mask = code_mask(bytes);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if word_at(bytes, &mask, index, "by") {
            let next = skip_space(bytes, index + 2, bytes.len());
            if bytes.get(next) == Some(&b'[') {
                replacements.push((index, index + 2, "key".to_owned()));
                index += 2;
                continue;
            }
        }
        if mask[index] && bytes[index].is_ascii_digit() {
            let capacity_end = integer_end(bytes, index, bytes.len());
            let slash = skip_space(bytes, capacity_end, bytes.len());
            if bytes.get(slash) == Some(&b'/') {
                let duration_start = skip_space(bytes, slash + 1, bytes.len());
                let duration_end = duration_literal_end(bytes, duration_start, bytes.len());
                let semicolon = skip_space(bytes, duration_end, bytes.len());
                if duration_end > duration_start && bytes.get(semicolon) == Some(&b';') {
                    replacements.push((
                        index,
                        semicolon,
                        format!(
                            "allow {} per {}",
                            contract[index..capacity_end].trim(),
                            contract[duration_start..duration_end].trim()
                        ),
                    ));
                    index = semicolon;
                    continue;
                }
            }
        }
        index += 1;
    }
    apply_replacements(contract, replacements)
}

fn rewrite_pagination_vocabulary(contract: &str) -> String {
    let bytes = contract.as_bytes();
    let mask = code_mask(bytes);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index + 8 <= bytes.len() {
        if !word_at(bytes, &mask, index, "paginate") {
            index += 1;
            continue;
        }
        let cursor = skip_space(bytes, index + 8, bytes.len());
        if word_at(bytes, &mask, cursor, "via") {
            index += 8;
            continue;
        }
        let Some(open) = find_top_level_byte(bytes, &mask, cursor, bytes.len(), b'{') else {
            index += 8;
            continue;
        };
        let Some(close) = matching_brace(bytes, &mask, open) else {
            index += 8;
            continue;
        };
        if word_at(bytes, &mask, cursor, "with") {
            replacements.push((cursor, cursor + 4, "via".to_owned()));
        } else {
            replacements.push((index + 8, index + 8, " via".to_owned()));
        }
        let fields = split_top_level(bytes, &mask, open + 1, close, b',');
        for (field_start, field_end) in fields {
            let field_start = skip_space(bytes, field_start, field_end);
            let field_end = trim_space_end(bytes, field_start, field_end);
            if field_start == field_end {
                continue;
            }
            let Some(colon) = find_top_level_byte(bytes, &mask, field_start, field_end, b':')
            else {
                continue;
            };
            let name = contract[field_start..colon].trim();
            let value_start = skip_space(bytes, colon + 1, field_end);
            let (operator, value_start) = if word_at(bytes, &mask, value_start, "bind") {
                ("->", skip_space(bytes, value_start + 4, field_end))
            } else {
                ("=", value_start)
            };
            let value = contract[value_start..field_end].trim();
            replacements.push((
                field_start,
                field_end,
                format!("{name} {operator} {value};"),
            ));
        }
        index = close + 1;
    }
    apply_replacements(contract, replacements)
}

fn apply_replacements(source: &str, mut replacements: Vec<(usize, usize, String)>) -> String {
    replacements.sort_by_key(|replacement| replacement.0);
    let mut output = source.to_owned();
    for (start, end, replacement) in replacements.into_iter().rev() {
        output.replace_range(start..end, &replacement);
    }
    output
}

fn word_at(bytes: &[u8], mask: &[bool], index: usize, word: &str) -> bool {
    index + word.len() <= bytes.len()
        && mask.get(index).copied().unwrap_or(false)
        && bytes[index..].starts_with(word.as_bytes())
        && boundary(bytes, index.checked_sub(1))
        && boundary(bytes, Some(index + word.len()))
}

fn top_level_delimiter(
    bytes: &[u8],
    mask: &[bool],
    start: usize,
    end: usize,
    delimiter: u8,
) -> Option<usize> {
    let (mut parentheses, mut brackets, mut braces, mut angles) = (0_i32, 0_i32, 0_i32, 0_i32);
    for index in start..end {
        if !mask[index] {
            continue;
        }
        if bytes[index] == delimiter
            && parentheses == 0
            && brackets == 0
            && braces == 0
            && angles == 0
        {
            return Some(index);
        }
        match bytes[index] {
            b'(' => parentheses += 1,
            b')' if parentheses > 0 => parentheses -= 1,
            b'[' => brackets += 1,
            b']' if brackets > 0 => brackets -= 1,
            b'{' => braces += 1,
            b'}' if braces > 0 => braces -= 1,
            b'<' => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            _ => {}
        }
    }
    None
}

fn find_top_level_byte(
    bytes: &[u8],
    mask: &[bool],
    start: usize,
    end: usize,
    target: u8,
) -> Option<usize> {
    top_level_delimiter(bytes, mask, start, end, target)
}

fn split_top_level(
    bytes: &[u8],
    mask: &[bool],
    start: usize,
    end: usize,
    delimiter: u8,
) -> Vec<(usize, usize)> {
    let mut output = Vec::new();
    let mut item_start = start;
    while item_start < end {
        if let Some(position) = top_level_delimiter(bytes, mask, item_start, end, delimiter) {
            output.push((item_start, position));
            item_start = position + 1;
        } else {
            output.push((item_start, end));
            break;
        }
    }
    output
}

fn integer_end(bytes: &[u8], start: usize, end: usize) -> usize {
    let mut cursor = start;
    while cursor < end && (bytes[cursor].is_ascii_digit() || bytes[cursor] == b'_') {
        cursor += 1;
    }
    cursor
}

fn duration_literal_end(bytes: &[u8], start: usize, end: usize) -> usize {
    let mut cursor = integer_end(bytes, start, end);
    while cursor < end && bytes[cursor].is_ascii_alphabetic() {
        cursor += 1;
    }
    cursor
}

fn signature_replacements(source: &str, start: usize, end: usize) -> Vec<(usize, usize, String)> {
    let bytes = source.as_bytes();
    let mask = code_mask(bytes);
    let mut replacements = Vec::new();
    let methods = [
        "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "METHOD",
    ];
    let mut index = start;
    while index < end {
        let Some(method) = methods.iter().find(|method| {
            index + method.len() <= end
                && mask[index]
                && bytes[index..].starts_with(method.as_bytes())
                && boundary(bytes, index.checked_sub(1))
                && boundary(bytes, Some(index + method.len()))
        }) else {
            index += 1;
            continue;
        };

        let mut cursor = skip_space(bytes, index + method.len(), end);
        if *method == "METHOD" {
            if bytes.get(cursor) != Some(&b'(') {
                index += method.len();
                continue;
            }
            let Some(close) = matching_delimiter(bytes, &mask, cursor, b'(', b')') else {
                index += method.len();
                continue;
            };
            cursor = skip_space(bytes, close + 1, end);
        }
        let Some(name_end) = rust_ident_end(bytes, cursor, end) else {
            index += method.len();
            continue;
        };
        cursor = skip_space(bytes, name_end, end);
        if bytes.get(cursor) != Some(&b'(') {
            index += method.len();
            continue;
        }
        let Some(parameters_end) = matching_delimiter(bytes, &mask, cursor, b'(', b')') else {
            index += method.len();
            continue;
        };
        replacements.extend(body_signature_replacements(
            source,
            &mask,
            cursor + 1,
            parameters_end,
        ));

        cursor = skip_space(bytes, parameters_end + 1, end);
        if bytes.get(cursor..cursor + 2) != Some(b"->") {
            index = parameters_end + 1;
            continue;
        }
        let type_start = skip_space(bytes, cursor + 2, end);
        let Some(type_end) = endpoint_output_end(bytes, &mask, type_start, end) else {
            index = parameters_end + 1;
            continue;
        };
        if !contains_top_level_word(bytes, &mask, type_start, type_end, "via") {
            let trimmed_end = trim_space_end(bytes, type_start, type_end);
            let specification = source[type_start..trimmed_end].trim();
            if !specification.is_empty() {
                replacements.push((
                    type_start,
                    trimmed_end,
                    format!("{OUTPUT_TYPE_REVIEW} via {specification}"),
                ));
            }
        }
        index = type_end.max(parameters_end + 1);
    }
    replacements.sort_by_key(|replacement| replacement.0);
    replacements
}

fn body_signature_replacements(
    source: &str,
    mask: &[bool],
    start: usize,
    end: usize,
) -> Vec<(usize, usize, String)> {
    let bytes = source.as_bytes();
    let mut replacements = Vec::new();
    let mut index = start;
    while index + 7 <= end {
        if !(mask[index] && bytes[index..].starts_with(b"#[body]")) {
            index += 1;
            continue;
        }
        let marker_start = index;
        let mut cursor = skip_space(bytes, index + 7, end);
        let Some(name_end) = rust_ident_end(bytes, cursor, end) else {
            index += 7;
            continue;
        };
        let name = &source[cursor..name_end];
        cursor = skip_space(bytes, name_end, end);
        if bytes.get(cursor) != Some(&b':') {
            index += 7;
            continue;
        }
        let type_start = skip_space(bytes, cursor + 1, end);
        let type_end = parameter_type_end(bytes, mask, type_start, end);
        let trimmed_end = trim_space_end(bytes, type_start, type_end);
        let specification = source[type_start..trimmed_end].trim();
        if !specification.is_empty() {
            replacements.push((
                marker_start,
                trimmed_end,
                format!("body {name}: {INPUT_TYPE_REVIEW} via {specification}"),
            ));
        }
        index = type_end.max(index + 7);
    }
    replacements
}

fn skip_space(bytes: &[u8], mut index: usize, end: usize) -> usize {
    while index < end && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn trim_space_end(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

fn rust_ident_end(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    if start >= end || !(bytes[start].is_ascii_alphabetic() || bytes[start] == b'_') {
        return None;
    }
    let mut cursor = start + 1;
    while cursor < end && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_') {
        cursor += 1;
    }
    Some(cursor)
}

fn matching_delimiter(
    bytes: &[u8],
    mask: &[bool],
    open: usize,
    opening: u8,
    closing: u8,
) -> Option<usize> {
    let mut depth = 0_u32;
    for index in open..bytes.len() {
        if !mask[index] {
            continue;
        }
        if bytes[index] == opening {
            depth = depth.checked_add(1)?;
        } else if bytes[index] == closing {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn parameter_type_end(bytes: &[u8], mask: &[bool], start: usize, end: usize) -> usize {
    nested_type_end(bytes, mask, start, end, b',')
}

fn endpoint_output_end(bytes: &[u8], mask: &[bool], start: usize, end: usize) -> Option<usize> {
    let semicolon = nested_type_end(bytes, mask, start, end, b';');
    let brace = nested_type_end(bytes, mask, start, end, b'{');
    let boundary = semicolon.min(brace);
    (boundary < end).then_some(boundary)
}

fn nested_type_end(bytes: &[u8], mask: &[bool], start: usize, end: usize, stop: u8) -> usize {
    let (mut parentheses, mut brackets, mut angles) = (0_i32, 0_i32, 0_i32);
    let mut index = start;
    while index < end {
        if !mask[index] {
            index += 1;
            continue;
        }
        match bytes[index] {
            b'(' => parentheses += 1,
            b')' if parentheses > 0 => parentheses -= 1,
            b'[' => brackets += 1,
            b']' if brackets > 0 => brackets -= 1,
            b'<' => angles += 1,
            b'>' if angles > 0 => angles -= 1,
            byte if byte == stop && parentheses == 0 && brackets == 0 && angles == 0 => {
                return index;
            }
            _ => {}
        }
        index += 1;
    }
    end
}

fn contains_top_level_word(
    bytes: &[u8],
    mask: &[bool],
    start: usize,
    end: usize,
    word: &str,
) -> bool {
    let word = word.as_bytes();
    let (mut parentheses, mut brackets, mut angles) = (0_i32, 0_i32, 0_i32);
    let mut index = start;
    while index + word.len() <= end {
        if mask[index]
            && parentheses == 0
            && brackets == 0
            && angles == 0
            && bytes[index..].starts_with(word)
            && boundary(bytes, index.checked_sub(1))
            && boundary(bytes, Some(index + word.len()))
        {
            return true;
        }
        if mask[index] {
            match bytes[index] {
                b'(' => parentheses += 1,
                b')' if parentheses > 0 => parentheses -= 1,
                b'[' => brackets += 1,
                b']' if brackets > 0 => brackets -= 1,
                b'<' => angles += 1,
                b'>' if angles > 0 => angles -= 1,
                _ => {}
            }
        }
        index += 1;
    }
    false
}

fn contract_macro_ranges(source: &str) -> Result<Vec<(usize, usize)>, String> {
    macro_ranges(source, "api!")
}

fn macro_ranges(source: &str, macro_name: &str) -> Result<Vec<(usize, usize)>, String> {
    let bytes = source.as_bytes();
    let mask = code_mask(bytes);
    let macro_name = macro_name.as_bytes();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index + macro_name.len() <= bytes.len() {
        if mask[index]
            && bytes[index..].starts_with(macro_name)
            && boundary(bytes, index.checked_sub(1))
            && boundary(bytes, Some(index + macro_name.len()))
        {
            let open = skip_space(bytes, index + macro_name.len(), bytes.len());
            if bytes.get(open) != Some(&b'{') {
                index += macro_name.len();
                continue;
            }
            let close = matching_brace(bytes, &mask, open).ok_or_else(|| {
                format!(
                    "{} invocation has an unmatched brace",
                    String::from_utf8_lossy(macro_name)
                )
            })?;
            ranges.push((open, close));
            index = close + 1;
        } else {
            index += 1;
        }
    }
    Ok(ranges)
}

fn client_range(source: &str, macro_open: usize, macro_close: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mask = code_mask(bytes);
    let mut depth = 0_i32;
    let mut index = macro_open + 1;
    while index + 6 <= macro_close {
        match bytes[index] {
            b'{' if mask[index] => depth += 1,
            b'}' if mask[index] => depth -= 1,
            _ => {}
        }
        if depth == 0
            && mask[index]
            && bytes[index..].starts_with(b"client")
            && boundary(bytes, index.checked_sub(1))
            && boundary(bytes, Some(index + 6))
        {
            let open = ((index + 6)..macro_close)
                .find(|candidate| mask[*candidate] && bytes[*candidate] == b'{')?;
            let close = matching_brace(bytes, &mask, open)?;
            return Some((open, close));
        }
        index += 1;
    }
    None
}

fn has_top_level_word(source: &str, start: usize, end: usize, word: &str) -> bool {
    let bytes = source.as_bytes();
    let mask = code_mask(bytes);
    let mut depth = 0_i32;
    let mut index = start;
    while index + word.len() <= end {
        match bytes[index] {
            b'{' if mask[index] => depth += 1,
            b'}' if mask[index] => depth -= 1,
            _ => {}
        }
        if depth == 0
            && mask[index]
            && bytes[index..].starts_with(word.as_bytes())
            && boundary(bytes, index.checked_sub(1))
            && boundary(bytes, Some(index + word.len()))
        {
            return true;
        }
        index += 1;
    }
    false
}

fn matching_brace(bytes: &[u8], mask: &[bool], open: usize) -> Option<usize> {
    let mut depth = 0_u32;
    for index in open..bytes.len() {
        if !mask[index] {
            continue;
        }
        match bytes[index] {
            b'{' => depth = depth.checked_add(1)?,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn boundary(bytes: &[u8], index: Option<usize>) -> bool {
    index.is_none_or(|index| {
        index >= bytes.len() || !(bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
    })
}

fn code_mask(bytes: &[u8]) -> Vec<bool> {
    #[derive(Clone, Copy)]
    enum State {
        Code,
        LineComment,
        BlockComment(u32),
        String(bool),
        Character(bool),
        RawString(usize),
    }

    let mut mask = vec![false; bytes.len()];
    let mut state = State::Code;
    let mut index = 0;
    while index < bytes.len() {
        match state {
            State::Code => {
                if bytes[index..].starts_with(b"//") {
                    state = State::LineComment;
                    index += 2;
                } else if bytes[index..].starts_with(b"/*") {
                    state = State::BlockComment(1);
                    index += 2;
                } else if let Some((hashes, consumed)) = raw_string_start(&bytes[index..]) {
                    state = State::RawString(hashes);
                    index += consumed;
                } else if bytes[index] == b'"' {
                    state = State::String(false);
                    index += 1;
                } else if bytes[index] == b'\'' && looks_like_character(&bytes[index..]) {
                    state = State::Character(false);
                    index += 1;
                } else {
                    mask[index] = true;
                    index += 1;
                }
            }
            State::LineComment => {
                if bytes[index] == b'\n' {
                    state = State::Code;
                    mask[index] = true;
                }
                index += 1;
            }
            State::BlockComment(depth) => {
                if bytes[index..].starts_with(b"/*") {
                    state = State::BlockComment(depth + 1);
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    state = if depth == 1 {
                        State::Code
                    } else {
                        State::BlockComment(depth - 1)
                    };
                    index += 2;
                } else {
                    index += 1;
                }
            }
            State::String(escaped) => {
                if escaped {
                    state = State::String(false);
                } else if bytes[index] == b'\\' {
                    state = State::String(true);
                } else if bytes[index] == b'"' {
                    state = State::Code;
                }
                index += 1;
            }
            State::Character(escaped) => {
                if escaped {
                    state = State::Character(false);
                } else if bytes[index] == b'\\' {
                    state = State::Character(true);
                } else if bytes[index] == b'\'' {
                    state = State::Code;
                }
                index += 1;
            }
            State::RawString(hashes) => {
                if bytes[index] == b'"'
                    && bytes
                        .get((index + 1)..(index + 1 + hashes))
                        .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
                {
                    state = State::Code;
                    index += 1 + hashes;
                } else {
                    index += 1;
                }
            }
        }
    }
    mask
}

fn code_only(source: &str) -> String {
    let mask = code_mask(source.as_bytes());
    let mut visible = source.as_bytes().to_vec();
    for (index, byte) in visible.iter_mut().enumerate() {
        if !mask[index] && !matches!(*byte, b'\r' | b'\n') {
            *byte = b' ';
        }
    }
    String::from_utf8(visible).expect("masking UTF-8 with ASCII spaces preserves UTF-8")
}

fn raw_string_start(bytes: &[u8]) -> Option<(usize, usize)> {
    let offset = usize::from(bytes.first().copied() == Some(b'b'));
    if bytes.get(offset).copied()? != b'r' {
        return None;
    }
    let mut hashes = 0;
    while bytes.get(offset + 1 + hashes).copied() == Some(b'#') {
        hashes += 1;
    }
    (bytes.get(offset + 1 + hashes).copied() == Some(b'"')).then_some((hashes, offset + 2 + hashes))
}

fn looks_like_character(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .enumerate()
        .skip(1)
        .take(12)
        .any(|(index, byte)| *byte == b'\'' && bytes.get(index - 1) != Some(&b'\\'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mechanical_rewrite_moves_children_and_reports_ambiguous_timeout() {
        let source = r#"
api! {
    client A { base "https://example.test/{ignored}"; }
    POST value(#[body] input: Text) -> Bytes { path ["}"]; }
}
"#;
        let (rewritten, moved, timeouts, signatures) = rewrite_mechanical(source).expect("rewrite");
        assert_eq!(moved, 1);
        assert_eq!(timeouts, 1);
        assert_eq!(signatures, 2);
        assert!(!rewritten.contains("timeout off;"));
        assert!(
            rewritten.contains("body input: __ConcordV2_1BodyInputTypeRequiresReview via Text")
        );
        assert!(
            rewritten.contains("__ConcordV2_1OutputTypeRequiresReview via Bytes"),
            "{rewritten}"
        );
        assert!(rewritten.find("POST value").unwrap() < rewritten.rfind('}').unwrap());
    }

    #[test]
    fn audit_view_ignores_retired_spellings_in_strings_and_comments() {
        let source = r#"
api! {
    client A {
        timeout 1s;
        base "https://scope.example.test";
        // ep.value and vars.value are migration prose, not syntax.
        GET read(value: Option<String>) -> String {
            id "runtime.scope.value";
            path [endpoint.value];
        }
    }
}
"#;
        let visible = code_only(source);
        assert!(!visible.contains("scope.example"));
        assert!(!visible.contains("runtime.scope"));
        assert!(!visible.contains("ep.value"));
        assert!(visible.contains("value: Option<String>"));
        assert!(visible.contains("endpoint.value"));
    }
}
