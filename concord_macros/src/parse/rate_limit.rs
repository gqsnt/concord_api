fn parse_rate_limit_profile_decl_after_keyword(
    input: ParseStream<'_>,
) -> Result<RateLimitProfileDef> {
    let name: Ident = input.parse()?;
    let extends = if input.peek(kw::extends) {
        input.parse::<kw::extends>()?;
        Some(input.parse()?)
    } else {
        None
    };
    let body;
    braced!(body in input);
    Ok(RateLimitProfileDef {
        name,
        extends,
        plan: parse_rate_limit_plan_body(&body)?,
    })
}

fn parse_rate_limit_spec(input: ParseStream<'_>) -> Result<RateLimitSpec> {
    input.parse::<kw::rate_limit>()?;

    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        return Ok(RateLimitSpec::Off);
    }

    let only = if input.peek(kw::only) {
        input.parse::<kw::only>()?;
        true
    } else {
        false
    };

    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        return Ok(RateLimitSpec::Inline {
            only,
            plan: parse_rate_limit_plan_body(&content)?,
        });
    }

    Ok(RateLimitSpec::Profiles {
        only,
        profiles: parse_rate_limit_profile_list(input)?,
    })
}

fn parse_rate_limit_key_binding(input: ParseStream<'_>) -> Result<RateLimitKeyBindingSpec> {
    input.parse::<kw::rate_limit>()?;
    input.parse::<kw::key>()?;
    let name: Ident = input.parse()?;
    input.parse::<Token![=]>()?;
    let value: Ident = input.parse()?;
    Ok(RateLimitKeyBindingSpec { name, value })
}

fn parse_rate_limit_profile_list(input: ParseStream<'_>) -> Result<Vec<Ident>> {
    if input.peek(token::Bracket) {
        let list_span = input.span();
        let content;
        bracketed!(content in input);
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        while !content.is_empty() {
            let ident: Ident = content.parse()?;
            if !seen.insert(ident.to_string()) {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("duplicate rate_limit profile `{ident}` in rate_limit list"),
                ));
            }
            out.push(ident);
            let _ = content.parse::<Option<Token![,]>>()?;
        }

        if out.is_empty() {
            return Err(syn::Error::new(
                list_span,
                "empty rate_limit list; expected at least one profile name",
            ));
        }

        return Ok(out);
    }

    Ok(vec![input.parse()?])
}

fn parse_rate_limit_plan_body(input: ParseStream<'_>) -> Result<RateLimitPlanSpec> {
    let mut plan = RateLimitPlanSpec::default();

    while !input.is_empty() {
        if input.peek(kw::bucket) {
            plan.buckets.push(parse_rate_limit_bucket(input)?);
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in rate_limit plan; expected `bucket`",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }

    Ok(plan)
}

fn parse_rate_limit_bucket(input: ParseStream<'_>) -> Result<RateLimitBucketSpec> {
    input.parse::<kw::bucket>()?;
    let kind: Ident = input.parse()?;
    input.parse::<kw::by>()?;
    let key = parse_rate_limit_key_list(input)?;
    let content;
    braced!(content in input);

    let mut cost = None;
    let mut windows = Vec::new();
    while !content.is_empty() {
        if content.peek(kw::cost) {
            if cost.is_some() {
                return Err(syn::Error::new(
                    content.span(),
                    "duplicate rate_limit bucket cost",
                ));
            }
            content.parse::<kw::cost>()?;
            cost = Some(content.parse::<LitInt>()?);
        } else if content.peek(LitInt) {
            let max: LitInt = content.parse()?;
            content.parse::<Token![/]>()?;
            let every: LitInt = content.parse()?;
            let suffix = every.suffix();
            let unit = match suffix {
                "s" => RateLimitDurationUnit::Seconds,
                "m" => RateLimitDurationUnit::Minutes,
                _ => {
                    return Err(syn::Error::new(
                        every.span(),
                        "rate_limit shorthand duration must use `s` or `m`, e.g. `500 / 10s`",
                    ));
                }
            };
            windows.push(RateLimitWindowSpec { max, every, unit });
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in rate_limit bucket; expected `cost` or `500 / 10s` shorthand",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(RateLimitBucketSpec {
        kind,
        key,
        cost,
        windows,
    })
}

fn parse_rate_limit_key_list(input: ParseStream<'_>) -> Result<Vec<RateLimitKeySpec>> {
    let content;
    bracketed!(content in input);
    let mut out = Vec::new();
    while !content.is_empty() {
        out.push(parse_rate_limit_key_spec(&content)?);
        let _ = content.parse::<Option<Token![,]>>()?;
    }
    Ok(out)
}

fn parse_rate_limit_key_spec(input: ParseStream<'_>) -> Result<RateLimitKeySpec> {
    if input.peek(LitStr) {
        return Ok(RateLimitKeySpec::Static(input.parse()?));
    }

    let first: Ident = input.parse()?;
    let first_s = first.to_string();
    match first_s.as_str() {
        "host" => Ok(RateLimitKeySpec::RouteHost),
        "endpoint" => Ok(RateLimitKeySpec::Endpoint),
        "method" => Ok(RateLimitKeySpec::Method),
        _ => Ok(RateLimitKeySpec::Named(first)),
    }
}

fn parse_rate_limit_duration_unit(input: ParseStream<'_>) -> Result<RateLimitDurationUnit> {
    if input.peek(kw::second) {
        input.parse::<kw::second>()?;
        Ok(RateLimitDurationUnit::Seconds)
    } else if input.peek(kw::seconds) {
        input.parse::<kw::seconds>()?;
        Ok(RateLimitDurationUnit::Seconds)
    } else if input.peek(kw::minute) {
        input.parse::<kw::minute>()?;
        Ok(RateLimitDurationUnit::Minutes)
    } else if input.peek(kw::minutes) {
        input.parse::<kw::minutes>()?;
        Ok(RateLimitDurationUnit::Minutes)
    } else {
        let tt: TokenTree = input.parse()?;
        Err(syn::Error::new(
            tt.span(),
            "expected rate_limit duration unit `second(s)` or `minute(s)`",
        ))
    }
}

