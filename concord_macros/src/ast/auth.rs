#[derive(Debug)]
pub struct AuthCredentials {
    pub credentials: Vec<AuthCredentialDecl>,
}

#[derive(Debug, Clone)]
pub struct AuthCredentialDecl {
    pub name: Ident,
    pub kind: AuthCredentialKind,
}

#[derive(Debug, Clone)]
pub enum AuthCredentialKind {
    ApiKey {
        secret: SecretRef,
    },
    StaticBearer {
        secret: SecretRef,
    },
    Basic {
        username: SecretRef,
        password: SecretRef,
    },
    OAuth2ClientCredentials {
        token_url: LitStr,
        client_id: SecretRef,
        client_secret: SecretRef,
        scope: Option<LitStr>,
    },
    Endpoint {
        endpoint: Path,
    },
}

#[derive(Debug, Clone)]
pub struct SecretRef {
    pub ident: Ident,
}

#[derive(Debug, Clone)]
pub enum AuthUseDecl {
    Single(Box<AuthUseKind>),
}

#[derive(Debug, Clone)]
pub enum AuthUseKind {
    Bearer {
        credential: Ident,
    },
    Header {
        header: LitStr,
        credential: Ident,
    },
    Query {
        key: LitStr,
        credential: Ident,
    },
    Basic {
        credential: Ident,
    },
    Certificate {
        credential: Ident,
    },
}

