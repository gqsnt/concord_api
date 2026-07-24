//! The single production boundary for sanitized metadata and diagnostics.

mod headers;
mod names;
mod url;

pub use headers::{SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders};
pub(crate) use url::render_safe_url;

#[cfg(test)]
mod tests {
    mod redaction_matrix;
    mod safe_url;
}
