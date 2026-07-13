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
        } else if input.peek(kw::retry) {
            return Err(removed_retry_syntax_error(input)?);
        } else if input.peek(kw::rate_limit) {
            if patch.rate_limit.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate rate_limit policy in profile",
                ));
            }
            patch.rate_limit = Some(parse_rate_limit_spec(input)?);
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "invalid item in profile; expected auth or rate_limit",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(patch)
}

fn parse_behavior_use_spec(input: ParseStream<'_>) -> Result<BehaviorUseSpec> {
    let span = input.span();
    input.parse::<kw::profile>()?;
    let names = if input.peek(token::Bracket) {
        let list_span = input.span();
        let content;
        bracketed!(content in input);
        let mut names = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        while !content.is_empty() {
            let name: Ident = content.parse()?;
            if !seen.insert(name.to_string()) {
                return Err(syn::Error::new(
                    name.span(),
                    format!("duplicate profile `{name}` in profile list"),
                ));
            }
            names.push(name);
            let _ = content.parse::<Option<Token![,]>>()?;
        }
        if names.is_empty() {
            return Err(syn::Error::new(
                list_span,
                "empty profile list; expected at least one profile name",
            ));
        }
        names
    } else {
        vec![input.parse()?]
    };
    Ok(BehaviorUseSpec { span, names })
}
