#[derive(Default, Clone)]
pub struct HostMap {
    /// Stocké dans l’ordre d’application (outer -> inner).
    /// L’affichage inverse l’ordre pour réaliser la sémantique "prefix inversé".
    pub segment: Vec<String>,
}

impl HostMap {
    pub fn from_vec(vec: Vec<String>) -> Self {
        Self { segment: vec }
    }

    pub fn push_label(&mut self, label: impl Into<String>) {
        self.segment.push(label.into());
    }

    pub fn is_empty(&self) -> bool {
        self.segment.is_empty()
    }

    pub fn join(&self, domain: &str) -> String {
        if self.segment.is_empty() {
            domain.to_string()
        } else {
            format!("{}.{}", self, domain)
        }
    }
}

impl std::fmt::Display for HostMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // reverse => sémantique "prefix inversé"
        for (i, seg) in self.segment.iter().rev().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", seg)?;
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

    /// Ajoute une portion de chemin, en normalisant les '/'.
    /// - accepte "/posts", "posts", "/posts/", etc.
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

    pub fn push_segment(&mut self, seg: &str) {
        self.push_raw(seg);
    }

    pub fn push_display<T: std::fmt::Display>(&mut self, v: T) {
        self.push_raw(&v.to_string());
    }
}

pub struct RouteParts {
    pub host: HostMap,
    pub path: UrlPath,
}

impl Default for RouteParts {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteParts {
    pub fn new() -> Self {
        Self {
            host: HostMap::default(),
            path: UrlPath::new(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_host_map_display_prefix_reverse() {
        // stocké dans l’ordre d’application (outer -> inner)
        let hm = HostMap::from_vec(vec![
            "com".into(),
            "example".into(),
            "v1".into(),
            "api".into(),
        ]);
        // affichage inverse
        assert_eq!(hm.to_string(), "api.v1.example.com");

        let hm_empty = HostMap::default();
        assert_eq!(hm_empty.join("example.com"), "example.com");
        assert_eq!(
            HostMap::from_vec(vec!["api".into()]).join("example.com"),
            "api.example.com"
        );
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
}
