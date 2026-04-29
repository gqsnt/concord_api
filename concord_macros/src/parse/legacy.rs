fn legacy_v5_error(span: Span, removed: &'static str, replacement: &'static str) -> syn::Error {
    syn::Error::new(span, format!("`{removed}` was removed in v5; use `{replacement}`"))
}

fn legacy_v5_renamed_error(
    span: Span,
    removed: &'static str,
    replacement: &'static str,
) -> syn::Error {
    syn::Error::new(span, format!("{removed} was renamed to {replacement} in v5"))
}

#[allow(dead_code)]
fn unsupported_v5_error(span: Span, feature: &'static str, guidance: &'static str) -> syn::Error {
    syn::Error::new(span, format!("{feature} is not supported in v5; {guidance}"))
}
