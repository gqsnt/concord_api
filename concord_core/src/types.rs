use crate::error::{ApiClientError, HostLabelInvalidReason};

#[derive(Clone, Debug)]
pub enum HostSpec {
    /// suffix-domain mode: labels + context DOMAIN.
    SuffixDomain { labels: Vec<HostLabel> },
    /// absolute host mode: full host already determined (no DOMAIN join).
    Absolute { host: String },
}

#[derive(Clone, Debug)]
pub struct HostLabel {
    pub value: String,
    pub source: HostLabelSource,
}

#[derive(Clone, Copy, Debug)]
pub enum HostLabelSource {
    Static(&'static str),
    Placeholder { name: &'static str },
    Mixed,
}

#[derive(Clone, Debug)]
pub struct HostParts {
    spec: HostSpec,
    absolute_push_attempt: Option<HostLabel>,
}

impl Default for HostParts {
    fn default() -> Self {
        Self {
            spec: HostSpec::SuffixDomain { labels: Vec::new() },
            absolute_push_attempt: None,
        }
    }
}

impl HostParts {
    #[inline]
    pub fn set_absolute(&mut self, host: impl Into<String>) {
        self.spec = HostSpec::Absolute { host: host.into() };
        self.absolute_push_attempt = None;
    }

    #[inline]
    pub fn push_label_static(&mut self, s: &'static str) {
        self.push_label(s.to_string(), HostLabelSource::Static(s));
    }

    #[inline]
    pub fn push_label(&mut self, value: impl Into<String>, source: HostLabelSource) {
        match &mut self.spec {
            HostSpec::SuffixDomain { labels } => labels.push(HostLabel {
                value: value.into(),
                source,
            }),
            HostSpec::Absolute { .. } => {
                // Ne pas ignorer silencieusement : on mémorise l'intention et validate() échouera.
                if self.absolute_push_attempt.is_none() {
                    self.absolute_push_attempt = Some(HostLabel { value: value.into(), source });
                }
            }
        }
    }

    pub fn validate(&self, endpoint: &'static str) -> Result<(), ApiClientError> {
        match &self.spec {
            HostSpec::Absolute { host } => {
                if let Some(lbl) = &self.absolute_push_attempt {
                    let placeholder = match lbl.source {
                        HostLabelSource::Placeholder { name } => Some(name),
                        _ => None,
                    };
                    return Err(ApiClientError::InvalidHostLabel {
                        endpoint,
                        label: lbl.value.clone(),
                        index: 0,
                        placeholder,
                        reason: HostLabelInvalidReason::AbsoluteModePushLabel,
                    });
                }
                let h = host.as_str();
                if h.is_empty() {
                    return Err(ApiClientError::InvalidHostLabel {
                        endpoint,
                        label: host.clone(),
                        index: 0,
                        placeholder: None,
                        reason: HostLabelInvalidReason::Empty,
                    });
                }
                if h.contains("://") {
                    return Err(ApiClientError::InvalidHostLabel {
                        endpoint,
                        label: host.clone(),
                        index: 0,
                        placeholder: None,
                        reason: HostLabelInvalidReason::ContainsScheme,
                    });
                }
                if h.chars().any(|c| c.is_whitespace()) {
                    return Err(ApiClientError::InvalidHostLabel {
                        endpoint,
                        label: host.clone(),
                        index: 0,
                        placeholder: None,
                        reason: HostLabelInvalidReason::ContainsWhitespace,
                    });
                }
                if h.contains('/') {
                    return Err(ApiClientError::InvalidHostLabel {
                        endpoint,
                        label: host.clone(),
                        index: 0,
                        placeholder: None,
                        reason: HostLabelInvalidReason::ContainsSlash,
                    });
                }
                Ok(())
            }
            HostSpec::SuffixDomain { labels } => {
                for (index, seg) in labels.iter().enumerate() {
                    let raw = seg.value.clone();
                    let s = raw.as_str();
                    let placeholder = match seg.source {
                        HostLabelSource::Placeholder { name } => Some(name),
                        _ => None,
                    };
                    if s.is_empty() {
                        return Err(ApiClientError::InvalidHostLabel {
                            endpoint,
                            label: raw,
                            index,
                            placeholder,
                            reason: HostLabelInvalidReason::Empty,
                        });
                    }
                    if s.contains('.') {
                        return Err(ApiClientError::InvalidHostLabel {
                            endpoint,
                            label: raw,
                            index,
                            placeholder,
                            reason: HostLabelInvalidReason::ContainsDot,
                        });
                    }
                    if s.bytes()
                        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C))
                    {
                        return Err(ApiClientError::InvalidHostLabel {
                            endpoint,
                            label: raw,
                            index,
                            placeholder,
                            reason: HostLabelInvalidReason::InvalidByte(b' '),
                        });
                    }
                    if s.contains('/') {
                        return Err(ApiClientError::InvalidHostLabel {
                            endpoint,
                            label: raw,
                            index,
                            placeholder,
                            reason: HostLabelInvalidReason::ContainsSlash,
                        });
                    }
                    if s.starts_with('-') || s.ends_with('-') {
                        return Err(ApiClientError::InvalidHostLabel {
                            endpoint,
                            label: raw,
                            index,
                            placeholder,
                            reason: HostLabelInvalidReason::StartsOrEndsDash,
                        });
                    }
                    for b in s.bytes() {
                        let ok = matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-');
                        if !ok {
                            return Err(ApiClientError::InvalidHostLabel {
                                endpoint,
                                label: raw,
                                index,
                                placeholder,
                                reason: HostLabelInvalidReason::InvalidByte(b),
                            });
                        }
                    }
                }
                Ok(())
            }
        }
    }

    pub fn join(&self, domain: &str) -> String {
        match &self.spec {
            HostSpec::Absolute { host } => host.clone(),
            HostSpec::SuffixDomain { labels } => {
                if labels.is_empty() {
                    domain.to_string()
                } else {
                    format!("{}.{}", HostPartsDisplay(labels), domain)
                }
            }
        }
    }
}

struct HostPartsDisplay<'a>(&'a [HostLabel]);
impl<'a> std::fmt::Display for HostPartsDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, seg) in self.0.iter().rev().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", seg.value)?;
        }
        Ok(())
    }
}

/// Builder de chemin URL (UTF-8, séparateur '/', pas de logique OS).
#[derive(Clone, Default)]
pub struct UrlPath {
    inner: String,
}

impl UrlPath {
    pub fn new() -> Self {
        Self {
            inner: "/".to_string(),
        }
    }

    pub fn as_str(&self) -> &str {
        if self.inner.is_empty() {
            "/"
        } else {
            self.inner.as_str()
        }
    }

    /// Contrat:
    /// - Les littéraux de route (issus des `"..."` côté macro) sont injectés **raw** via cette méthode.
    /// - Cette injection est normalisée (jointure + retrait du trailing slash), mais **autorise** des '/' dans la chaîne.
    /// - Ne pas utiliser avec des données utilisateur.
    /// - Pour des données utilisateur, utiliser `push_segment_encoded()` (utilisé par les `{placeholder}`).
    pub fn push_raw(&mut self, piece: &str) {
        let piece = piece.trim();
        if piece.is_empty() || piece == "/" {
            return;
        }

        if self.inner.is_empty() {
            self.inner.push('/');
        } else if !self.inner.starts_with('/') {
            self.inner.insert(0, '/');
        }

        let left_slash = self.inner.ends_with('/');
        let right_slash = piece.starts_with('/');

        match (left_slash, right_slash) {
            (true, true) => {
                // évite "//"
                self.inner.pop();
                self.inner.push_str(piece);
            }
            (false, false) => {
                self.inner.push('/');
                self.inner.push_str(piece);
            }
            _ => {
                self.inner.push_str(piece);
            }
        }

        // Retire trailing slash sauf si root
        if self.inner.len() > 1 && self.inner.ends_with('/') {
            self.inner.pop();
        }
    }

    fn percent_encode_path_segment(seg: &str) -> String {
        // RFC3986 "unreserved": ALPHA / DIGIT / "-" / "." / "_" / "~"
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let bytes = seg.as_bytes();
        let mut out = String::with_capacity(bytes.len());
        for &b in bytes {
            let unreserved = matches!(
              b,
              b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
            );
            if unreserved {
                out.push(b as char);
            } else {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0F) as usize] as char);
            }
        }
        out
    }

    /// Contrat:
    /// - Utilisé pour les `{placeholder}` (valeurs dynamiques).
    /// - Garantit une sémantique "un segment" (pas de '/').
    pub fn push_segment_encoded(&mut self, seg: &str) {
        if seg.is_empty() || seg == "/" {
            return;
        }
        // segment semantics: ignore leading/trailing slashes coming from formatting
        let seg = seg.trim_matches('/');
        if seg.is_empty() {
            return;
        }
        let enc = Self::percent_encode_path_segment(seg);
        self.push_raw(&enc);
    }

    pub fn push_segment(&mut self, seg: &str) {
        self.push_raw(seg);
    }

    pub fn push_display<T: std::fmt::Display>(&mut self, v: T) {
        self.push_raw(&v.to_string());
    }
}

pub struct RouteParts {
    host: HostParts,
    path: UrlPath,
}

impl Default for RouteParts {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteParts {
    pub fn new() -> Self {
        Self {
            host: HostParts::default(),
            path: UrlPath::new(),
        }
    }

    pub fn path_mut(&mut self) -> &mut UrlPath {
        &mut self.path
    }

    pub fn host_mut(&mut self) -> &mut HostParts {
        &mut self.host
    }
    pub fn host(&self) -> &HostParts {
        &self.host
    }
    pub fn path(&self) -> &UrlPath {
        &self.path
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_host_label_order_is_stable() {
        let mut h = HostParts::default();
        h.push_label_static("v1");
        h.push_label_static("api");
        assert_eq!(h.join("example.com"), "api.v1.example.com");
    }
    #[test]
    fn test_url_path_push() {
        let mut p = UrlPath::new();
        assert_eq!(p.as_str(), "/");

        p.push_raw("posts");
        assert_eq!(p.as_str(), "/posts");

        p.push_raw("/1");
        assert_eq!(p.as_str(), "/posts/1");

        p.push_raw("comments/");
        assert_eq!(p.as_str(), "/posts/1/comments");
    }

    #[test]
    fn test_url_path_segment_encoded_does_not_trim_spaces() {
        let mut p = UrlPath::new();
        p.push_segment_encoded(" a ");
        assert_eq!(p.as_str(), "/%20a%20");
    }

    #[test]
    fn test_host_parts_absolute_rejects_push_label() {
        let mut h = HostParts::default();
        h.set_absolute("api.example.net");
        h.push_label_static("v1");
        let err = h.validate("TestEndpoint").unwrap_err();
        match err {
            ApiClientError::InvalidHostLabel { reason, .. } => {
                assert!(matches!(reason, HostLabelInvalidReason::AbsoluteModePushLabel));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
