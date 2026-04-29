enum RetryDecl {
    Spec(RetrySpec),
}

fn parse_retry_decl(input: ParseStream<'_>) -> Result<RetryDecl> {
    input.parse::<kw::retry>()?;

    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        return Ok(RetryDecl::Spec(RetrySpec::Off));
    }

    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        if content.peek(kw::profile) || content.peek(kw::default) {
            return Err(syn::Error::new(
                content.span(),
                "`retry { profile ... }` was removed in v5; declare `retry name { ... }` in the client block",
            ));
        }
        return Ok(RetryDecl::Spec(RetrySpec::Patch(parse_retry_patch_body(
            &content,
        )?)));
    }

    Ok(RetryDecl::Spec(RetrySpec::Profile(input.parse()?)))
}

fn parse_retry_profile_decl_after_keyword(input: ParseStream<'_>) -> Result<RetryProfileDef> {
    let name: Ident = input.parse()?;
    let extends = if input.peek(kw::extends) {
        input.parse::<kw::extends>()?;
        Some(input.parse()?)
    } else {
        None
    };
    let content;
    braced!(content in input);
    Ok(RetryProfileDef {
        name,
        extends,
        patch: parse_retry_patch_body(&content)?,
    })
}

fn parse_retry_patch_body(input: ParseStream<'_>) -> Result<RetryPatch> {
    let mut patch = RetryPatch::default();

    while !input.is_empty() {
        if input.peek(kw::attempts) {
            input.parse::<kw::attempts>()?;
            return Err(legacy_v5_renamed_error(
                input.span(),
                "`attempts`",
                "`max_attempts`",
            ));
        } else if input.peek(kw::max_attempts) {
            input.parse::<kw::max_attempts>()?;
            set_retry_patch_field(
                &mut patch.max_attempts,
                input.parse::<LitInt>()?,
                input.span(),
                "max_attempts",
            )?;
        } else if input.peek(kw::methods) {
            input.parse::<kw::methods>()?;
            let methods = parse_ident_list(input)?;
            set_retry_patch_field(&mut patch.methods, methods, input.span(), "methods")?;
        } else if input.peek(kw::on) {
            input.parse::<kw::on>()?;
            if input.peek(token::Bracket) {
                let statuses = parse_lit_int_list(input)?;
                set_retry_patch_field(&mut patch.statuses, statuses, input.span(), "status")?;
            } else if input.peek(kw::transport) {
                input.parse::<kw::transport>()?;
                let transport_errors = parse_ident_list(input)?;
                set_retry_patch_field(
                    &mut patch.transport_errors,
                    transport_errors,
                    input.span(),
                    "transport",
                )?;
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `[...]` or `transport[...]` after `on`",
                ));
            }
        } else if input.peek(kw::retry_after) {
            input.parse::<kw::retry_after>()?;
            set_retry_patch_field(
                &mut patch.respect_retry_after,
                true,
                input.span(),
                "retry_after",
            )?;
        } else if input.peek(kw::idempotency) {
            input.parse::<kw::idempotency>()?;
            if input.peek(kw::header) {
                input.parse::<kw::header>()?;
                let header: LitStr = input.parse()?;
                set_retry_patch_field(
                    &mut patch.idempotency,
                    RetryIdempotencySpec::Header(header),
                    input.span(),
                    "idempotency",
                )?;
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `header(\"...\")` after `idempotency`",
                ));
            }
        } else if input.peek(kw::backoff) {
            return Err(syn::Error::new(
                input.span(),
                "`backoff none` was removed in v5; remove the line",
            ));
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in retry policy block",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }

    Ok(patch)
}

fn set_retry_patch_field<T>(
    out: &mut Option<T>,
    value: T,
    span: Span,
    field: &'static str,
) -> Result<()> {
    if out.is_some() {
        return Err(syn::Error::new(
            span,
            format!("duplicate retry `{field}` field"),
        ));
    }
    *out = Some(value);
    Ok(())
}

fn parse_ident_list(input: ParseStream<'_>) -> Result<Vec<Ident>> {
    let content;
    bracketed!(content in input);
    let mut out = Vec::new();
    while !content.is_empty() {
        out.push(content.parse()?);
        let _ = content.parse::<Option<Token![,]>>()?;
    }
    Ok(out)
}

fn parse_lit_int_list(input: ParseStream<'_>) -> Result<Vec<LitInt>> {
    let content;
    bracketed!(content in input);
    let mut out = Vec::new();
    while !content.is_empty() {
        out.push(content.parse()?);
        let _ = content.parse::<Option<Token![,]>>()?;
    }
    Ok(out)
}

