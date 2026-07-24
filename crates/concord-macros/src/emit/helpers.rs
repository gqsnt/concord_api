use super::*;

pub(super) fn method_token(method: &ResolvedMethod, core_path: &TokenStream) -> TokenStream {
    match method {
        ResolvedMethod::Get => quote!(#core_path::__private::Method::GET),
        ResolvedMethod::Post => quote!(#core_path::__private::Method::POST),
        ResolvedMethod::Put => quote!(#core_path::__private::Method::PUT),
        ResolvedMethod::Patch => quote!(#core_path::__private::Method::PATCH),
        ResolvedMethod::Delete => quote!(#core_path::__private::Method::DELETE),
        ResolvedMethod::Head => quote!(#core_path::__private::Method::HEAD),
        ResolvedMethod::Options => quote!(#core_path::__private::Method::OPTIONS),
        ResolvedMethod::Trace => quote!(#core_path::__private::Method::TRACE),
        ResolvedMethod::Connect => quote!(#core_path::__private::Method::CONNECT),
        ResolvedMethod::Custom(value) => quote!(
            #core_path::__private::Method::from_bytes(#value.as_bytes())
                .expect("the Concord macro validates custom HTTP method tokens")
        ),
    }
}

pub(super) fn request_body_input_type(body: &TokenStream, core_path: &TokenStream) -> TokenStream {
    quote!(<#body as #core_path::advanced::RequestBodySpec>::Input)
}

pub(super) fn default_expr(value: &ResolvedDefault) -> TokenStream {
    match value {
        ResolvedDefault::String(value) => quote!(#value),
        ResolvedDefault::Integer { negative, literal } if *negative => quote!(-#literal),
        ResolvedDefault::Integer { literal, .. } => quote!(#literal),
        ResolvedDefault::Boolean(value) => quote!(#value),
        ResolvedDefault::Path(value) => quote!(#value),
    }
}

pub(super) fn super_chain(count: usize) -> TokenStream {
    let supers = (0..count).map(|_| quote!(super)).collect::<Vec<_>>();
    quote!(#(#supers)::* )
}

pub(super) fn module_ancestor_path(count: usize) -> TokenStream {
    if count == 0 {
        quote!(self)
    } else {
        super_chain(count)
    }
}

pub(super) fn helper_ident(prefix: &str, name: &Ident) -> Ident {
    public_ident(&format!("{prefix}_{}", canonical_ident(name)), name.span())
}

pub(super) fn client_value_field(index: usize, value: &Value) -> Ident {
    private_ident("client_value", index, &value.name)
}

pub(super) fn builder_value_field(index: usize, value: &Value) -> Ident {
    private_ident("builder_value", index, &value.name)
}

pub(super) fn resolved_client_value(index: usize, value: &Value) -> Ident {
    private_ident("resolved_client_value", index, &value.name)
}

pub(super) fn scope_value_field(scope_id: usize, index: usize, value: &Value) -> Ident {
    Ident::new(
        &format!(
            "__concord_scope_{scope_id}_{index}_{}",
            to_snake_case(&canonical_ident(&value.name))
        ),
        Span::mixed_site(),
    )
}

pub(super) fn scope_default_field(scope_id: usize, index: usize, value: &Value) -> Ident {
    Ident::new(
        &format!(
            "__concord_scope_default_{scope_id}_{index}_{}",
            to_snake_case(&canonical_ident(&value.name))
        ),
        Span::mixed_site(),
    )
}

pub(super) fn endpoint_value_field(index: usize, value: &Value) -> Ident {
    private_ident("endpoint_value", index, &value.name)
}

pub(super) fn endpoint_default_field(index: usize, value: &Value) -> Ident {
    private_ident("endpoint_default", index, &value.name)
}

pub(super) fn credentials_store_name(client: &Client) -> Ident {
    Ident::new(
        &format!(
            "__Concord{}Credentials",
            crate::semantic::surface::to_pascal_case(&canonical_ident(&client.name))
        ),
        Span::mixed_site(),
    )
}

pub(super) fn credentials_facade_name(client: &Client) -> Ident {
    Ident::new(
        &format!(
            "{}Credentials",
            crate::semantic::surface::to_pascal_case(&canonical_ident(&client.name))
        ),
        client.name.span(),
    )
}

pub(super) fn secret_builder_field(index: usize, name: &Ident) -> Ident {
    private_ident("builder_secret", index, name)
}

pub(super) fn resolved_secret(index: usize, name: &Ident) -> Ident {
    private_ident("resolved_secret", index, name)
}

pub(super) fn provider_builder_field(index: usize, name: &Ident) -> Ident {
    private_ident("builder_provider", index, name)
}

pub(super) fn resolved_provider(index: usize, name: &Ident) -> Ident {
    private_ident("resolved_provider", index, name)
}

pub(super) fn credential_store_field(index: usize, name: &Ident) -> Ident {
    private_ident("credential", index, name)
}

pub(super) fn provider_argument_name(name: &Ident) -> Ident {
    public_ident(
        &format!("{}_provider", to_snake_case(&canonical_ident(name))),
        name.span(),
    )
}

pub(super) fn private_ident(namespace: &str, index: usize, name: &Ident) -> Ident {
    Ident::new(
        &format!(
            "__concord_{namespace}_{index}_{}",
            to_snake_case(&canonical_ident(name))
        ),
        Span::mixed_site(),
    )
}
