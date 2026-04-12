use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CredentialId {
    namespace: &'static str,
    name: &'static str,
}

impl CredentialId {
    #[inline]
    pub const fn new(namespace: &'static str, name: &'static str) -> Self {
        Self { namespace, name }
    }

    #[inline]
    pub fn namespace(&self) -> &'static str {
        self.namespace
    }

    #[inline]
    pub fn name(&self) -> &'static str {
        self.name
    }

    #[inline]
    pub fn safe_fragment(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }
}

impl fmt::Display for CredentialId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.namespace, self.name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthUsageId(&'static str);

impl AuthUsageId {
    #[inline]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    #[inline]
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for AuthUsageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum AuthIdentity {
    Anonymous,
    Static(&'static str),
    User(String),
    Tenant(String),
    ScopeAudience {
        scope: Vec<String>,
        audience: Option<String>,
    },
    OpaqueHash(String),
}

impl AuthIdentity {
    #[inline]
    pub fn safe_fragment(&self) -> String {
        match self {
            Self::Anonymous => "anon".to_string(),
            Self::Static(v) => format!("static:{v}"),
            Self::User(v) => format!("user:{v}"),
            Self::Tenant(v) => format!("tenant:{v}"),
            Self::ScopeAudience { scope, audience } => {
                format!("scope:{};aud:{:?}", scope.join(","), audience)
            }
            Self::OpaqueHash(v) => format!("hash:{v}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthProvenance {
    pub layer: &'static str,
}

impl AuthProvenance {
    #[inline]
    pub const fn new(layer: &'static str) -> Self {
        Self { layer }
    }
}

impl Default for AuthProvenance {
    fn default() -> Self {
        Self::new("runtime")
    }
}
