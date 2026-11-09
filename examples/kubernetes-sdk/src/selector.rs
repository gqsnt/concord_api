//! Label and field selectors, built from typed requirements and rendered in the Kubernetes
//! selector syntax.
//!
//! A selector's [`Display`](std::fmt::Display) is its selector text (`app=web,tier in (api,db)`);
//! Concord places it as the `labelSelector` / `fieldSelector` query value and percent-encodes it
//! like every query value (`=` is `%3D`, `,` is `%2C`, space is `+`). Selector text is never
//! assembled by string concatenation in caller code.
//!
//! - **Labels** ([`LabelSelector`]): equality (`=`, `!=`), set (`in`, `notin`) and existence
//!   (`key`, `!key`) requirements, comma-joined (logical AND). Keys and values are validated
//!   against the label syntax (`k8s.io/apimachinery/pkg/util/validation`: a key is an optional
//!   DNS-subdomain prefix and `/`, then a name of at most 63 bytes of `[A-Za-z0-9._-]` starting and
//!   ending alphanumeric; a value is empty or follows the name rule), so no value can inject
//!   selector syntax.
//! - **Fields** ([`FieldSelector`]): `field=value` and `field!=value` terms. Values are escaped as
//!   `fields.EscapeValue` does (`\` → `\\`, `,` → `\,`, `=` → `\=`), so arbitrary text (an object
//!   name) cannot split a term. Which fields a resource supports is decided by the server
//!   (`metadata.name`, `metadata.namespace` everywhere; `status.phase`, `spec.nodeName` on Pods).

use std::fmt;

/// Why a selector requirement was refused. Names the rule, never the offending text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_selector_error)]
pub enum SelectorError {
    /// A label key is empty, too long or uses invalid characters.
    InvalidLabelKey,
    /// A label value is too long or uses invalid characters.
    InvalidLabelValue,
    /// An `in`/`notin` requirement has no values.
    EmptySet,
    /// A field path is empty or uses characters other than `[A-Za-z0-9._-]`.
    InvalidFieldPath,
}

fn display_selector_error(
    value: &SelectorError,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    f.write_str(match value {
        SelectorError::InvalidLabelKey => "invalid label key",
        SelectorError::InvalidLabelValue => "invalid label value",
        SelectorError::EmptySet => "a set requirement needs at least one value",
        SelectorError::InvalidFieldPath => "invalid field path",
    })
}

/// `[A-Za-z0-9]([-A-Za-z0-9_.]*[A-Za-z0-9])?`, at most 63 bytes (a label name or value).
fn is_qualified_name_part(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 63
        && b[0].is_ascii_alphanumeric()
        && b[b.len() - 1].is_ascii_alphanumeric()
        && b.iter()
            .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

/// A DNS-1123 subdomain (a label key prefix).
fn is_dns_subdomain(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && s.split('.').all(|label| {
            let b = label.as_bytes();
            !b.is_empty()
                && b.len() <= 63
                && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
                && (b[b.len() - 1].is_ascii_lowercase() || b[b.len() - 1].is_ascii_digit())
                && b.iter()
                    .all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
}

fn check_key(key: &str) -> Result<(), SelectorError> {
    let ok = match key.split_once('/') {
        Some((prefix, name)) => is_dns_subdomain(prefix) && is_qualified_name_part(name),
        None => is_qualified_name_part(key),
    };
    if ok {
        Ok(())
    } else {
        Err(SelectorError::InvalidLabelKey)
    }
}

fn check_value(value: &str) -> Result<(), SelectorError> {
    if value.is_empty() || is_qualified_name_part(value) {
        Ok(())
    } else {
        Err(SelectorError::InvalidLabelValue)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Requirement {
    Equals(String, String),
    NotEquals(String, String),
    In(String, Vec<String>),
    NotIn(String, Vec<String>),
    Exists(String),
    DoesNotExist(String),
}

/// A label selector: requirements joined by `,` (all must hold). Build it with
/// [`LabelSelector::builder`].
///
/// ```
/// use concord_example_kubernetes_sdk::LabelSelector;
///
/// let selector = LabelSelector::builder()
///     .equals("app.kubernetes.io/name", "web")
///     .in_set("tier", ["frontend", "api"])
///     .not_exists("canary")
///     .build()?;
/// assert_eq!(
///     selector.to_string(),
///     "app.kubernetes.io/name=web,tier in (api,frontend),!canary"
/// );
/// # Ok::<(), concord_example_kubernetes_sdk::SelectorError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelSelector {
    requirements: Vec<Requirement>,
}

impl LabelSelector {
    /// An empty builder.
    pub fn builder() -> LabelSelectorBuilder {
        LabelSelectorBuilder {
            requirements: Vec::new(),
            error: None,
        }
    }

    /// `key=value`, the most common selector.
    ///
    /// # Errors
    ///
    /// [`SelectorError`] for an invalid key or value.
    pub fn equals(key: &str, value: &str) -> Result<Self, SelectorError> {
        Self::builder().equals(key, value).build()
    }

    /// Whether `labels` satisfy every requirement (used to apply the same selector to objects a
    /// watch reports, and in tests to check what a server returned).
    pub fn matches(&self, labels: &std::collections::BTreeMap<String, String>) -> bool {
        self.requirements.iter().all(|r| match r {
            Requirement::Equals(k, v) => labels.get(k) == Some(v),
            Requirement::NotEquals(k, v) => labels.get(k) != Some(v),
            Requirement::In(k, vs) => labels.get(k).is_some_and(|v| vs.contains(v)),
            Requirement::NotIn(k, vs) => labels.get(k).is_none_or(|v| !vs.contains(v)),
            Requirement::Exists(k) => labels.contains_key(k),
            Requirement::DoesNotExist(k) => !labels.contains_key(k),
        })
    }
}

impl fmt::Display for LabelSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, r) in self.requirements.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            match r {
                Requirement::Equals(k, v) => write!(f, "{k}={v}")?,
                Requirement::NotEquals(k, v) => write!(f, "{k}!={v}")?,
                Requirement::In(k, vs) => write!(f, "{k} in ({})", vs.join(","))?,
                Requirement::NotIn(k, vs) => write!(f, "{k} notin ({})", vs.join(","))?,
                Requirement::Exists(k) => write!(f, "{k}")?,
                Requirement::DoesNotExist(k) => write!(f, "!{k}")?,
            }
        }
        Ok(())
    }
}

/// Builds a [`LabelSelector`]; the first invalid requirement is reported by
/// [`build`](Self::build).
#[derive(Clone, Debug)]
#[must_use]
pub struct LabelSelectorBuilder {
    requirements: Vec<Requirement>,
    error: Option<SelectorError>,
}

impl LabelSelectorBuilder {
    fn push(mut self, check: Result<(), SelectorError>, r: impl FnOnce() -> Requirement) -> Self {
        match (self.error, check) {
            (None, Ok(())) => self.requirements.push(r()),
            (None, Err(e)) => self.error = Some(e),
            (Some(_), _) => {}
        }
        self
    }

    /// `key=value`.
    pub fn equals(self, key: &str, value: &str) -> Self {
        let check = check_key(key).and_then(|()| check_value(value));
        self.push(check, || Requirement::Equals(key.into(), value.into()))
    }

    /// `key!=value` (also matches objects without the label).
    pub fn not_equals(self, key: &str, value: &str) -> Self {
        let check = check_key(key).and_then(|()| check_value(value));
        self.push(check, || Requirement::NotEquals(key.into(), value.into()))
    }

    /// `key in (v1,v2)`; values are sorted and deduplicated, as Kubernetes renders them.
    pub fn in_set<'a>(self, key: &str, values: impl IntoIterator<Item = &'a str>) -> Self {
        let (check, set) = set_of(key, values);
        self.push(check, || Requirement::In(key.into(), set))
    }

    /// `key notin (v1,v2)` (also matches objects without the label).
    pub fn not_in_set<'a>(self, key: &str, values: impl IntoIterator<Item = &'a str>) -> Self {
        let (check, set) = set_of(key, values);
        self.push(check, || Requirement::NotIn(key.into(), set))
    }

    /// `key`: the label is present, whatever its value.
    pub fn exists(self, key: &str) -> Self {
        self.push(check_key(key), || Requirement::Exists(key.into()))
    }

    /// `!key`: the label is absent.
    pub fn not_exists(self, key: &str) -> Self {
        self.push(check_key(key), || Requirement::DoesNotExist(key.into()))
    }

    /// The selector.
    ///
    /// # Errors
    ///
    /// The first [`SelectorError`] among the requirements.
    pub fn build(self) -> Result<LabelSelector, SelectorError> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(LabelSelector {
                requirements: self.requirements,
            }),
        }
    }
}

fn set_of<'a>(
    key: &str,
    values: impl IntoIterator<Item = &'a str>,
) -> (Result<(), SelectorError>, Vec<String>) {
    let mut set: Vec<String> = values.into_iter().map(str::to_owned).collect();
    set.sort();
    set.dedup();
    let check = check_key(key).and_then(|()| {
        if set.is_empty() {
            return Err(SelectorError::EmptySet);
        }
        set.iter().try_for_each(|v| check_value(v))
    });
    (check, set)
}

/// A field selector: `field=value` / `field!=value` terms joined by `,`.
///
/// ```
/// use concord_example_kubernetes_sdk::FieldSelector;
///
/// let selector = FieldSelector::builder()
///     .equals("status.phase", "Running")
///     .not_equals("metadata.name", "a,b=c")
///     .build()?;
/// assert_eq!(selector.to_string(), r"status.phase=Running,metadata.name!=a\,b\=c");
/// # Ok::<(), concord_example_kubernetes_sdk::SelectorError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSelector {
    terms: Vec<(String, bool, String)>,
}

impl FieldSelector {
    /// An empty builder.
    pub fn builder() -> FieldSelectorBuilder {
        FieldSelectorBuilder {
            terms: Vec::new(),
            error: None,
        }
    }
}

/// `fields.EscapeValue`: backslash, comma and equals sign are escaped with a backslash.
fn escape_field_value(value: &str, out: &mut fmt::Formatter<'_>) -> fmt::Result {
    for c in value.chars() {
        match c {
            '\\' => out.write_str(r"\\")?,
            ',' => out.write_str(r"\,")?,
            '=' => out.write_str(r"\=")?,
            c => fmt::Write::write_char(out, c)?,
        }
    }
    Ok(())
}

impl fmt::Display for FieldSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (field, equal, value)) in self.terms.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            f.write_str(field)?;
            f.write_str(if *equal { "=" } else { "!=" })?;
            escape_field_value(value, f)?;
        }
        Ok(())
    }
}

/// Builds a [`FieldSelector`].
#[derive(Clone, Debug)]
#[must_use]
pub struct FieldSelectorBuilder {
    terms: Vec<(String, bool, String)>,
    error: Option<SelectorError>,
}

impl FieldSelectorBuilder {
    fn term(mut self, field: &str, equal: bool, value: &str) -> Self {
        let valid = !field.is_empty()
            && field
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'));
        if self.error.is_none() {
            if valid {
                self.terms.push((field.into(), equal, value.into()));
            } else {
                self.error = Some(SelectorError::InvalidFieldPath);
            }
        }
        self
    }

    /// `field=value`.
    pub fn equals(self, field: &str, value: &str) -> Self {
        self.term(field, true, value)
    }

    /// `field!=value`.
    pub fn not_equals(self, field: &str, value: &str) -> Self {
        self.term(field, false, value)
    }

    /// The selector.
    ///
    /// # Errors
    ///
    /// [`SelectorError::InvalidFieldPath`] for the first invalid field path.
    pub fn build(self) -> Result<FieldSelector, SelectorError> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(FieldSelector { terms: self.terms }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_requirements_render_in_kubernetes_syntax() {
        let s = LabelSelector::builder()
            .equals("app", "web")
            .not_equals("env", "prod")
            .in_set("tier", ["db", "api", "db"])
            .not_in_set("zone", ["b", "a"])
            .exists("example.com/team")
            .not_exists("canary")
            .build()
            .unwrap();
        assert_eq!(
            s.to_string(),
            "app=web,env!=prod,tier in (api,db),zone notin (a,b),example.com/team,!canary"
        );
        // An empty value is a valid label value (`key=`).
        assert_eq!(LabelSelector::equals("k", "").unwrap().to_string(), "k=");
    }

    #[test]
    fn label_syntax_cannot_be_injected() {
        let bad = |b: LabelSelectorBuilder| b.build().unwrap_err();
        assert_eq!(
            bad(LabelSelector::builder().equals("app", "web,env=prod")),
            SelectorError::InvalidLabelValue
        );
        assert_eq!(
            bad(LabelSelector::builder().equals("a b", "x")),
            SelectorError::InvalidLabelKey
        );
        assert_eq!(
            bad(LabelSelector::builder().equals("Example.com/x", "y")),
            SelectorError::InvalidLabelKey
        );
        assert_eq!(
            bad(LabelSelector::builder().exists("")),
            SelectorError::InvalidLabelKey
        );
        assert_eq!(
            bad(LabelSelector::builder().in_set("k", [])),
            SelectorError::EmptySet
        );
        assert_eq!(
            bad(LabelSelector::builder().equals("k", &"v".repeat(64))),
            SelectorError::InvalidLabelValue
        );
        // The first error wins; later valid requirements do not clear it.
        assert_eq!(
            bad(LabelSelector::builder().equals("k", "-x").equals("a", "b")),
            SelectorError::InvalidLabelValue
        );
    }

    #[test]
    fn label_selector_matches_like_the_server() {
        let labels = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        let s = LabelSelector::builder()
            .equals("app", "web")
            .not_in_set("tier", ["db"])
            .not_exists("canary")
            .build()
            .unwrap();
        assert!(s.matches(&labels(&[("app", "web")])));
        assert!(s.matches(&labels(&[("app", "web"), ("tier", "api")])));
        assert!(!s.matches(&labels(&[("app", "web"), ("tier", "db")])));
        assert!(!s.matches(&labels(&[("app", "web"), ("canary", "")])));
        assert!(!s.matches(&labels(&[("app", "api")])));
    }

    #[test]
    fn field_values_are_escaped_like_fields_escape_value() {
        // Vector from k8s.io/apimachinery/pkg/fields: EscapeValue escapes `\`, `,` and `=`.
        let s = FieldSelector::builder()
            .equals("metadata.name", r"a,b=c\d")
            .build()
            .unwrap();
        assert_eq!(s.to_string(), r"metadata.name=a\,b\=c\\d");
        assert_eq!(
            FieldSelector::builder().equals("a b", "x").build(),
            Err(SelectorError::InvalidFieldPath)
        );
        assert_eq!(
            FieldSelector::builder().equals("", "x").build(),
            Err(SelectorError::InvalidFieldPath)
        );
    }
}
