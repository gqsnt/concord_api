use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::MetadataCommand;
use concord_core::__private::{
    DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS, DEFAULT_PAGINATION_IDENTITY_BYTES,
    DEFAULT_PAGINATION_MAX_ITEMS, DEFAULT_PAGINATION_MAX_PAGES, DEFAULT_PROVIDER_REQUEST_BYTES,
    DEFAULT_PROVIDER_RESPONSE_BYTES, DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS,
    DEFAULT_RATE_LIMIT_KEY_BYTES, DEFAULT_REQUEST_BODY_BYTES, DEFAULT_RESPONSE_BODY_BYTES,
    DEFAULT_STATUS_BODY_BYTES, DEFAULT_STREAM_RESPONSE_BYTES,
};
use concord_core::contracts::{
    MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTH_PLACEMENTS, MAX_AUTH_QUERY_VALUE_BYTES,
    MAX_AUTHORIZATION_VALUE_BYTES, MAX_BASIC_PASSWORD_BYTES, MAX_BASIC_USERNAME_BYTES,
    MAX_CREDENTIAL_SECRET_BYTES, MAX_EMPTY_REQUEST_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS,
    MAX_ENDPOINT_ID_BYTES, MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_IDENTITY_PEM_BYTES,
    MAX_MULTIPART_CONTENT_TYPE_BYTES, MAX_MULTIPART_FILENAME_BYTES,
    MAX_MULTIPART_INLINE_TEXT_BYTES, MAX_MULTIPART_METADATA_BYTES, MAX_MULTIPART_NAME_BYTES,
    MAX_MULTIPART_PARTS, MAX_PROXY_URL_BYTES, MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES,
    MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_RULES_PER_PLAN,
    MAX_RATE_LIMIT_WINDOWS_PER_RULE, MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES,
    MAX_REQUEST_HEADER_COUNT, MAX_REQUEST_URL_BYTES, MAX_ROOT_CERTIFICATE_BYTES,
    MAX_ROOT_CERTIFICATES, MAX_ROOT_CERTIFICATES_TOTAL_BYTES, MAX_RUNTIME_ENDPOINT_COMPONENTS,
    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, MAX_SECRET_STRING_BYTES,
};
use concord_extras::auth::{
    MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES, MAX_OAUTH2_CLIENT_ID_BYTES,
    MAX_OAUTH2_CLIENT_SECRET_BYTES, MAX_OAUTH2_SCOPE_BYTES, MAX_OAUTH2_TOKEN_URL_BYTES,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

const GENERATED_FEATURES_START: &str = "<!-- BEGIN GENERATED FEATURES -->";
const GENERATED_FEATURES_END: &str = "<!-- END GENERATED FEATURES -->";
const GENERATED_LIMITS_START: &str = "<!-- BEGIN GENERATED LIMITS -->";
const GENERATED_LIMITS_END: &str = "<!-- END GENERATED LIMITS -->";

const FEATURES: &[(&str, &str)] = &[
    (
        "default",
        "Default TLS, HTTP/2, and process-local rate limiting",
    ),
    ("default-tls", "HTTPS with the ordinary default TLS backend"),
    ("http2", "HTTP/2 transport support"),
    (
        "rate-limit",
        "Process-local generated rate-plan enforcement",
    ),
    ("json", "`Json<T>` request and response codec"),
    ("oauth2-client", "OAuth2 client-credentials provider helper"),
    ("records", "Generic bounded record streaming"),
    (
        "csv",
        "Type-configurable CSV record decoder (also enables `records`)",
    ),
    ("ndjson", "NDJSON record decoder (also enables `records`)"),
    ("multipart", "Managed multipart request construction"),
    ("gzip", "Gzip response decompression"),
    ("brotli", "Brotli response decompression"),
    ("deflate", "Deflate response decompression"),
    (
        "dangerous-raw-response",
        "Bounded undecoded successful-response terminal",
    ),
    (
        "dangerous-custom-retry",
        "Custom native transport retry policy",
    ),
];

struct Limit {
    label: &'static str,
    value: u64,
    unit: &'static str,
}

pub(crate) fn run(root: &Path, write: bool) -> Result<(), String> {
    synchronize_generated_section(
        &root.join("docs/reference/features.md"),
        GENERATED_FEATURES_START,
        GENERATED_FEATURES_END,
        &generated_feature_table(root)?,
        write,
    )?;
    synchronize_generated_section(
        &root.join("docs/reference/limits.md"),
        GENERATED_LIMITS_START,
        GENERATED_LIMITS_END,
        &generated_limit_table(),
        write,
    )?;
    check_markdown_links(root)?;
    println!("documentation checks passed");
    Ok(())
}

fn generated_feature_table(root: &Path) -> Result<String, String> {
    let mut command = MetadataCommand::new();
    command
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .other_options(vec!["--locked".into(), "--offline".into()]);
    let metadata = command
        .exec()
        .map_err(|error| format!("could not read Cargo metadata: {error}"))?;
    let facade = metadata
        .packages
        .iter()
        .find(|package| package.name.as_str() == "concord")
        .ok_or_else(|| "Cargo metadata omitted the concord façade".to_owned())?;
    let actual = facade
        .features
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected = FEATURES
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "documented façade features differ from Cargo metadata: expected {expected:?}, found {actual:?}"
        ));
    }
    let defaults = facade
        .features
        .get("default")
        .ok_or_else(|| "concord has no default feature declaration".to_owned())?
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let mut table =
        String::from("| Feature | Enabled by `default` | Capability |\n|---|---:|---|\n");
    for (name, description) in FEATURES {
        let enabled = if *name == "default" {
            "meta"
        } else if defaults.contains(name) {
            "yes"
        } else {
            "no"
        };
        table.push_str(&format!("| `{name}` | {enabled} | {description} |\n"));
    }
    Ok(table)
}

fn generated_limit_table() -> String {
    let mut table = String::from("| Bound | Value | Unit |\n|---|---:|---|\n");
    for limit in documented_limits() {
        table.push_str(&format!(
            "| {} | {} | {} |\n",
            limit.label,
            grouped(limit.value),
            limit.unit
        ));
    }
    table
}

#[allow(
    clippy::too_many_lines,
    reason = "the table is the complete finite-limit inventory"
)]
fn documented_limits() -> Vec<Limit> {
    vec![
        Limit {
            label: "Request body",
            value: DEFAULT_REQUEST_BODY_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Empty chunks in one managed request body",
            value: MAX_EMPTY_REQUEST_CHUNKS as u64,
            unit: "chunks",
        },
        Limit {
            label: "Buffered successful response",
            value: DEFAULT_RESPONSE_BODY_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Streaming response",
            value: DEFAULT_STREAM_RESPONSE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Empty transport chunks in one acquired response body",
            value: MAX_EMPTY_RESPONSE_CHUNKS as u64,
            unit: "chunks",
        },
        Limit {
            label: "Captured terminal status body",
            value: DEFAULT_STATUS_BODY_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Credential-provider request body",
            value: DEFAULT_PROVIDER_REQUEST_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Credential-provider response body",
            value: DEFAULT_PROVIDER_RESPONSE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Pagination pages",
            value: u64::from(DEFAULT_PAGINATION_MAX_PAGES),
            unit: "pages",
        },
        Limit {
            label: "Pagination accumulated items",
            value: DEFAULT_PAGINATION_MAX_ITEMS as u64,
            unit: "items",
        },
        Limit {
            label: "Retained encoded pagination identities",
            value: DEFAULT_PAGINATION_IDENTITY_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Authentication-failure cooldown cap",
            value: DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS,
            unit: "seconds",
        },
        Limit {
            label: "Server-directed rate-limit cooldown cap",
            value: DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS,
            unit: "seconds",
        },
        Limit {
            label: "Default limiter-prepared rate-limit key",
            value: DEFAULT_RATE_LIMIT_KEY_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "One formatted public value",
            value: MAX_FORMATTED_PUBLIC_VALUE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Final managed request head",
            value: MAX_REQUEST_HEAD_COMPONENTS as u64,
            unit: "components",
        },
        Limit {
            label: "Logical or credential-sanitized request URL",
            value: MAX_REQUEST_URL_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Public request headers before authentication",
            value: MAX_REQUEST_HEADER_COUNT as u64,
            unit: "headers",
        },
        Limit {
            label: "Aggregate public request headers",
            value: MAX_REQUEST_HEADER_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Proxy URL text",
            value: MAX_PROXY_URL_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Custom root certificates",
            value: MAX_ROOT_CERTIFICATES as u64,
            unit: "certificates",
        },
        Limit {
            label: "One custom root certificate PEM",
            value: MAX_ROOT_CERTIFICATE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Aggregate custom root certificate PEMs",
            value: MAX_ROOT_CERTIFICATES_TOTAL_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Application identity PEM",
            value: MAX_IDENTITY_PEM_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Endpoint contract identity",
            value: MAX_ENDPOINT_ID_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Authentication placement name",
            value: MAX_AUTH_PLACEMENT_NAME_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Generic protected string",
            value: MAX_SECRET_STRING_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Raw credential secret",
            value: MAX_CREDENTIAL_SECRET_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Basic authentication username",
            value: MAX_BASIC_USERNAME_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Basic authentication password",
            value: MAX_BASIC_PASSWORD_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Native authentication header value",
            value: MAX_AUTHORIZATION_VALUE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Encoded authentication query value",
            value: MAX_AUTH_QUERY_VALUE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Multipart recipe",
            value: MAX_MULTIPART_PARTS as u64,
            unit: "parts",
        },
        Limit {
            label: "Multipart part name",
            value: MAX_MULTIPART_NAME_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Multipart filename",
            value: MAX_MULTIPART_FILENAME_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Multipart part content type",
            value: MAX_MULTIPART_CONTENT_TYPE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Multipart aggregate metadata",
            value: MAX_MULTIPART_METADATA_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Multipart inline text",
            value: MAX_MULTIPART_INLINE_TEXT_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Runtime endpoint top-level structure",
            value: MAX_RUNTIME_ENDPOINT_COMPONENTS as u64,
            unit: "components",
        },
        Limit {
            label: "Runtime endpoint aggregate public shape",
            value: MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Authentication plan",
            value: MAX_AUTH_PLACEMENTS as u64,
            unit: "placements",
        },
        Limit {
            label: "Rate-limit rule ID",
            value: MAX_RATE_LIMIT_ID_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Rate-limit key",
            value: MAX_RATE_LIMIT_KEY_PARTS as u64,
            unit: "parts",
        },
        Limit {
            label: "Rate-limit key-part name",
            value: MAX_RATE_LIMIT_KEY_PART_NAME_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Rate-limit hard aggregate key",
            value: MAX_RATE_LIMIT_KEY_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "Rate-limit rule",
            value: MAX_RATE_LIMIT_WINDOWS_PER_RULE as u64,
            unit: "windows",
        },
        Limit {
            label: "Rate-limit plan",
            value: MAX_RATE_LIMIT_RULES_PER_PLAN as u64,
            unit: "rules",
        },
        Limit {
            label: "OAuth2 token URL text",
            value: MAX_OAUTH2_TOKEN_URL_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "OAuth2 client identifier",
            value: MAX_OAUTH2_CLIENT_ID_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "OAuth2 client secret",
            value: MAX_OAUTH2_CLIENT_SECRET_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "OAuth2 scope",
            value: MAX_OAUTH2_SCOPE_BYTES as u64,
            unit: "bytes",
        },
        Limit {
            label: "OAuth2 Basic Authorization value",
            value: MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES as u64,
            unit: "bytes",
        },
    ]
}

fn synchronize_generated_section(
    path: &Path,
    start: &str,
    end: &str,
    generated: &str,
    write: bool,
) -> Result<(), String> {
    let current = read(path)?;
    let expected = replace_section(&current, start, end, generated)?;
    if current == expected {
        return Ok(());
    }
    if write {
        fs::write(path, expected).map_err(|error| format!("{}: {error}", path.display()))?;
        println!("updated {}", path.display());
        Ok(())
    } else {
        Err(format!(
            "{} is stale; run `cargo run --locked -p concord-xtask -- docs --write`",
            path.display()
        ))
    }
}

fn replace_section(
    current: &str,
    start: &str,
    end: &str,
    generated: &str,
) -> Result<String, String> {
    let start_offset = current
        .find(start)
        .ok_or_else(|| format!("missing marker {start}"))?;
    let content_start = start_offset + start.len();
    let content_end = content_start
        + current[content_start..]
            .find(end)
            .ok_or_else(|| format!("missing marker {end}"))?;
    let mut output = String::with_capacity(current.len() + generated.len());
    output.push_str(&current[..content_start]);
    output.push('\n');
    output.push_str(generated.trim_end());
    output.push('\n');
    output.push_str(&current[content_end..]);
    Ok(output)
}

fn check_markdown_links(root: &Path) -> Result<(), String> {
    let mut paths = vec![root.join("README.md")];
    paths.extend(markdown_files(&root.join("docs"))?);
    let mut parsed = BTreeMap::<PathBuf, ParsedMarkdown>::new();
    for path in &paths {
        parsed.insert(path.clone(), parse_markdown(path)?);
    }

    let mut local_files = 0_usize;
    let mut local_anchors = 0_usize;
    let mut external_urls = 0_usize;
    for path in paths {
        let links = parsed
            .get(&path)
            .expect("every Markdown path was parsed above")
            .links
            .clone();
        for target in links {
            if target.contains("://") || target.starts_with("mailto:") {
                external_urls += 1;
                continue;
            }
            let (file_target, anchor) = target
                .split_once('#')
                .map_or((target.as_str(), None), |(file, anchor)| {
                    (file, Some(anchor))
                });
            let resolved = if file_target.is_empty() {
                path.clone()
            } else if let Some(repository_path) = file_target.strip_prefix('/') {
                root.join(repository_path)
            } else {
                path.parent().unwrap_or(root).join(file_target)
            };
            if !resolved.exists() {
                return Err(format!(
                    "{} links to missing `{file_target}`",
                    path.display()
                ));
            }
            local_files += usize::from(!file_target.is_empty());
            let Some(anchor) = anchor.filter(|anchor| !anchor.is_empty()) else {
                continue;
            };
            if resolved.extension() != Some(OsStr::new("md")) {
                continue;
            }
            let target_document = match parsed.get(&resolved) {
                Some(document) => document,
                None => {
                    parsed.insert(resolved.clone(), parse_markdown(&resolved)?);
                    parsed
                        .get(&resolved)
                        .expect("inserted Markdown target is available")
                }
            };
            if !target_document.anchors.contains(anchor) {
                return Err(format!(
                    "{} links to missing anchor `#{anchor}` in {}",
                    path.display(),
                    resolved.display()
                ));
            }
            local_anchors += 1;
        }
    }
    println!(
        "markdown links checked: {local_files} local file links, {local_anchors} local anchors; {external_urls} external URLs were syntax-counted but not fetched"
    );
    Ok(())
}

#[derive(Default)]
struct ParsedMarkdown {
    links: Vec<String>,
    anchors: BTreeSet<String>,
}

fn parse_markdown(path: &Path) -> Result<ParsedMarkdown, String> {
    let text = read(path)?;
    let mut parsed = ParsedMarkdown::default();
    let mut heading = None::<String>;
    let mut occurrences = BTreeMap::<String, usize>::new();
    for event in Parser::new_ext(&text, Options::all()) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                parsed.links.push(dest_url.into_string());
            }
            Event::Start(Tag::Heading { .. }) => heading = Some(String::new()),
            Event::End(TagEnd::Heading(_)) => {
                if let Some(heading) = heading.take() {
                    let base = github_heading_anchor(&heading);
                    if !base.is_empty() {
                        let occurrence = occurrences.entry(base.clone()).or_default();
                        let anchor = if *occurrence == 0 {
                            base
                        } else {
                            format!("{base}-{}", *occurrence)
                        };
                        *occurrence += 1;
                        parsed.anchors.insert(anchor);
                    }
                }
            }
            Event::Text(text) | Event::Code(text) if heading.is_some() => {
                heading
                    .as_mut()
                    .expect("heading is present")
                    .push_str(&text);
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                collect_explicit_html_anchors(&html, &mut parsed.anchors);
            }
            _ => {}
        }
    }
    Ok(parsed)
}

fn github_heading_anchor(heading: &str) -> String {
    heading
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_') {
                Some(character)
            } else if character.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

fn collect_explicit_html_anchors(html: &str, anchors: &mut BTreeSet<String>) {
    for prefix in ["id=\"", "name=\"", "id='", "name='"] {
        let quote = if prefix.ends_with('"') { '"' } else { '\'' };
        let mut rest = html;
        while let Some(start) = rest.find(prefix) {
            rest = &rest[start + prefix.len()..];
            let Some(end) = rest.find(quote) else {
                break;
            };
            if end > 0 {
                anchors.insert(rest[..end].to_owned());
            }
            rest = &rest[end + 1..];
        }
    }
}

fn markdown_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_markdown(directory, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_markdown(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            collect_markdown(&path, files)?;
        } else if path.extension() == Some(OsStr::new("md")) {
            files.push(path);
        }
    }
    Ok(())
}

fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{github_heading_anchor, replace_section};

    #[test]
    fn generated_sections_replace_only_owned_content() {
        let current = "before\n<!-- start -->\nold\n<!-- end -->\nafter\n";
        let replaced =
            replace_section(current, "<!-- start -->", "<!-- end -->", "new\n").expect("markers");
        assert_eq!(
            replaced,
            "before\n<!-- start -->\nnew\n<!-- end -->\nafter\n"
        );
    }

    #[test]
    fn heading_anchors_match_github_shape() {
        assert_eq!(
            github_heading_anchor("Public API: `Request<T>`"),
            "public-api-requestt"
        );
    }
}
