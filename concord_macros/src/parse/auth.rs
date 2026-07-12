fn parse_auth_credential_after_keyword(
    input: ParseStream<'_>,
    _requires_equals: bool,
) -> Result<AuthCredentialDecl> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;

        let kind_name: Ident = input.parse()?;
        let kind = match kind_name.to_string().as_str() {
            "api_key" => {
                let content;
                parenthesized!(content in input);
                let secret = parse_secret_ref(&content)?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected ApiKey arguments",
                    ));
                }
                AuthCredentialKind::ApiKey { secret }
            }
            "bearer" => {
                let content;
                parenthesized!(content in input);
                let secret = parse_secret_ref(&content)?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected bearer token arguments",
                    ));
                }
                AuthCredentialKind::StaticBearer { secret }
            }
            "basic" => {
                let content;
                parenthesized!(content in input);
                let username = parse_secret_ref(&content)?;
                content.parse::<Token![,]>()?;
                let password = parse_secret_ref(&content)?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected Basic arguments",
                    ));
                }
                AuthCredentialKind::Basic { username, password }
            }
            "oauth2_client" => {
                parse_oauth2_client_credentials(input, kind_name.span())?.into()
            }
            "endpoint" => {
                let endpoint = parse_auth_endpoint_ref(input)?;
                AuthCredentialKind::Endpoint { endpoint }
            }
            _ => {
                return Err(syn::Error::new(
                    kind_name.span(),
                    "unknown auth credential kind; expected api_key(...), bearer(...), basic(...), oauth2_client { ... }, or endpoint path",
                ));
            }
        };

        Ok(AuthCredentialDecl { name, kind })
}

fn parse_auth_endpoint_ref(input: ParseStream<'_>) -> Result<Path> {
    let mut segments = syn::punctuated::Punctuated::<syn::PathSegment, Token![::]>::new();
    let first: Ident = input.parse()?;
    segments.push(syn::PathSegment::from(first));

    while input.peek(Token![.]) || input.peek(Token![::]) {
        if input.peek(Token![.]) {
            input.parse::<Token![.]>()?;
        } else {
            input.parse::<Token![::]>()?;
        }
        let ident: Ident = input.parse()?;
        segments.push(syn::PathSegment::from(ident));
    }

    let path = Path {
        leading_colon: None,
        segments,
    };
    validate_auth_endpoint_path(&path)?;
    Ok(path)
}

struct OAuth2ClientCredentialsFields {
    token_url: LitStr,
    client_id: SecretRef,
    client_secret: SecretRef,
    scope: Option<LitStr>,
}

fn parse_oauth2_client_credentials(
    input: ParseStream<'_>,
    span: Span,
) -> Result<OAuth2ClientCredentialsFields> {
    let content;
    braced!(content in input);

    let mut token_url: Option<LitStr> = None;
    let mut client_id: Option<SecretRef> = None;
    let mut client_secret: Option<SecretRef> = None;
    let mut scope: Option<LitStr> = None;

    while !content.is_empty() {
        let key: Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        match key.to_string().as_str() {
            "token_url" => set_once_lit(&mut token_url, key.span(), content.parse()?)?,
            "client_id" => {
                set_once_secret_ref(&mut client_id, key.span(), parse_secret_ref(&content)?)?
            }
            "client_secret" => {
                set_once_secret_ref(&mut client_secret, key.span(), parse_secret_ref(&content)?)?
            }
            "scope" => set_once_lit(&mut scope, key.span(), content.parse()?)?,
            _ => {
                return Err(syn::Error::new(
                    key.span(),
                    "unknown OAuth2ClientCredentials field; expected token_url, client_id, client_secret, or scope",
                ));
            }
        }

        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        } else if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between OAuth2ClientCredentials fields",
            ));
        }
    }

    Ok(OAuth2ClientCredentialsFields {
        token_url: token_url
            .ok_or_else(|| syn::Error::new(span, "OAuth2ClientCredentials missing `token_url`"))?,
        client_id: client_id
            .ok_or_else(|| syn::Error::new(span, "OAuth2ClientCredentials missing `client_id`"))?,
        client_secret: client_secret.ok_or_else(|| {
            syn::Error::new(span, "OAuth2ClientCredentials missing `client_secret`")
        })?,
        scope,
    })
}

impl From<OAuth2ClientCredentialsFields> for AuthCredentialKind {
    fn from(v: OAuth2ClientCredentialsFields) -> Self {
        AuthCredentialKind::OAuth2ClientCredentials {
            token_url: v.token_url,
            client_id: v.client_id,
            client_secret: v.client_secret,
            scope: v.scope,
        }
    }
}

fn set_once_lit(out: &mut Option<LitStr>, span: Span, value: LitStr) -> Result<()> {
    if out.is_some() {
        return Err(syn::Error::new(span, "duplicate auth field"));
    }
    *out = Some(value);
    Ok(())
}

fn set_once_secret_ref(out: &mut Option<SecretRef>, span: Span, value: SecretRef) -> Result<()> {
    if out.is_some() {
        return Err(syn::Error::new(span, "duplicate auth field"));
    }
    *out = Some(value);
    Ok(())
}

fn parse_secret_ref(input: ParseStream<'_>) -> Result<SecretRef> {
    let ns: Ident = input.parse()?;
    if ns != "secret" {
        return Err(syn::Error::new(
            ns.span(),
            "auth credentials must reference secrets as `secret.name`",
        ));
    }
    input.parse::<Token![.]>()?;
    let ident: Ident = input.parse()?;
    Ok(SecretRef { ident })
}

fn validate_auth_endpoint_path(path: &Path) -> Result<()> {
    if path.segments.is_empty() {
        return Err(syn::Error::new_spanned(
            path,
            "Endpoint(...) expects `Login` or `scope::Login`",
        ));
    }
    for segment in &path.segments {
        if !matches!(segment.arguments, syn::PathArguments::None) {
            return Err(syn::Error::new_spanned(
                segment,
                "Endpoint(...) path segments must not contain generic arguments",
            ));
        }
    }
    Ok(())
}

fn parse_auth_use_decl_after_auth_keyword(input: ParseStream<'_>) -> Result<AuthUseDecl> {
    Ok(AuthUseDecl::Single(Box::new(parse_auth_use_kind(input)?)))
}

fn parse_auth_use_kind(input: ParseStream<'_>) -> Result<AuthUseKind> {
    let usage: Ident = input.parse()?;
    match usage.to_string().as_str() {
        "none" | "any" | "all" => Err(syn::Error::new(
            usage.span(),
            "auth none/any/all are not supported; declare explicit credential usage",
        )),
        "bearer" => Ok(AuthUseKind::Bearer {
            credential: input.parse()?,
        }),
        "header" => {
            let header: LitStr = input.parse()?;
            input.parse::<Token![=]>()?;
            let credential: Ident = input.parse()?;
            Ok(AuthUseKind::Header { header, credential })
        }
        "query" => {
            let key: LitStr = input.parse()?;
            input.parse::<Token![=]>()?;
            let credential: Ident = input.parse()?;
            Ok(AuthUseKind::Query { key, credential })
        }
        "basic" => Ok(AuthUseKind::Basic {
            credential: input.parse()?,
        }),
        _ => Err(syn::Error::new(
            usage.span(),
            "unknown auth usage; expected `bearer credential`, `header \"Name\" = credential`, `query \"name\" = credential`, or `basic credential`",
        )),
    }
}

