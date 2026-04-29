fn unsupported_auth_group_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "auth any/all groups are not supported in v5; write multiple auth lines for required auth",
    )
}

fn unsupported_custom_auth_credential_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "custom auth credentials are not supported in v5 yet; implement a CredentialProvider plus bearer/header/query/basic/certificate placement instead",
    )
}

fn unsupported_custom_auth_placement_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "custom auth placement is not supported in v5; use bearer/header/query/basic/certificate placement instead",
    )
}
