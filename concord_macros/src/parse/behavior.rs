fn parse_behavior_profile_decl_after_keyword(input: ParseStream<'_>) -> Result<BehaviorProfileDef> {
    let name: Ident = input.parse()?;
    let extends = if input.peek(kw::extends) {
        input.parse::<kw::extends>()?; 
        Some(input.parse()?)
    } else {
        None
    };
    let content;
    braced!(content in input);
    Ok(BehaviorProfileDef {
        name,
        extends,
        patch: parse_behavior_patch_body(&content)?,
    })
}

fn parse_behavior_patch_body(input: ParseStream<'_>) -> Result<BehaviorPatch> {
    let mut patch = BehaviorPatch::default();
    while !input.is_empty() {
        if input.peek(kw::auth) {
            input.parse::<kw::auth>()?;
            patch.auth_uses.push(parse_auth_use_decl_after_auth_keyword(input)?);
        } else if input.peek(kw::cache) {
            if patch.cache.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate cache policy in behavior",
                ));
            }
            match parse_cache_decl(input)? {
                CacheDecl::Spec(spec) => patch.cache = Some(spec),
            }
        } else if input.peek(kw::retry) {
            if patch.retry.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate retry policy in behavior",
                ));
            }
            match parse_retry_decl(input)? {
                RetryDecl::Spec(spec) => patch.retry = Some(spec),
            }
        } else if input.peek(kw::rate_limit) {
            if patch.rate_limit.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate rate_limit policy in behavior",
                ));
            }
            patch.rate_limit = Some(parse_rate_limit_spec(input)?);
        } else {
            let tt: TokenTree = input.parse()?; 
            return Err(syn::Error::new(tt.span(), "invalid item in behavior"));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(patch)
}

fn parse_behavior_use_spec(input: ParseStream<'_>) -> Result<BehaviorUseSpec> {
    let span = input.span();
    input.parse::<kw::behavior>()?;
    let names = if input.peek(token::Bracket) {
        let content;
        bracketed!(content in input);
        let mut names = Vec::new();
        while !content.is_empty() {
            names.push(content.parse()?);
            let _ = content.parse::<Option<Token![,]>>()?;
        }
        names
    } else {
        vec![input.parse()?]
    };
    Ok(BehaviorUseSpec { span, names })
}
