enum CacheDecl {
    Spec(CacheSpec),
}

fn parse_cache_decl(input: ParseStream<'_>) -> Result<CacheDecl> {
    Ok(CacheDecl::Spec(parse_cache_spec(input)?))
}

fn parse_cache_profile_decl_after_keyword(input: ParseStream<'_>) -> Result<CacheProfileDef> {
    let name: Ident = input.parse()?;
    let extends = if input.peek(kw::extends) {
        input.parse::<kw::extends>()?;
        Some(input.parse()?)
    } else {
        None
    };
    let body;
    braced!(body in input);
    Ok(CacheProfileDef {
        name,
        extends,
        patch: parse_cache_patch_body(&body)?,
    })
}

fn parse_cache_spec(input: ParseStream<'_>) -> Result<CacheSpec> {
    input.parse::<kw::cache>()?;
    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        return Ok(CacheSpec::Off);
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
        return Ok(CacheSpec::Patch {
            only,
            patch: parse_cache_patch_body(&content)?,
        });
    }

    if input.peek(kw::http) {
        let token = input.parse::<kw::http>()?;
        return Ok(CacheSpec::Patch {
            only,
            patch: CachePatch {
                http: Some(token.span),
                ..CachePatch::default()
            },
        });
    }

    if input.peek(kw::revalidate) {
        input.parse::<kw::revalidate>()?;
        return Ok(CacheSpec::Patch {
            only,
            patch: CachePatch {
                revalidate: Some(LitBool::new(true, input.span())),
                ..CachePatch::default()
            },
        });
    }

    if input.peek(kw::stale_on_error) {
        input.parse::<kw::stale_on_error>()?;
        return Ok(CacheSpec::Patch {
            only,
            patch: CachePatch {
                on_error: Some(CacheOnErrorSpec::ServeStale),
                ..CachePatch::default()
            },
        });
    }

    if input.peek(LitInt) {
        let amount: LitInt = input.parse()?;
        let unit = parse_rate_limit_duration_unit_from_lit_or_stream(&amount, input)?;
        return Ok(CacheSpec::Patch {
            only,
            patch: CachePatch {
                ttl: Some(CacheDurationSpec { amount, unit }),
                ..CachePatch::default()
            },
        });
    }

    Ok(CacheSpec::Profile {
        only,
        profile: input.parse()?,
    })
}

fn parse_cache_patch_body(input: ParseStream<'_>) -> Result<CachePatch> {
    let mut patch = CachePatch::default();
    while !input.is_empty() {
        if input.peek(kw::http) {
            if patch.http.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache http mode"));
            }
            let token = input.parse::<kw::http>()?;
            patch.http = Some(token.span);
        } else if input.peek(kw::ttl) {
            if patch.ttl.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache ttl"));
            }
            input.parse::<kw::ttl>()?;
            let amount: LitInt = input.parse()?;
            let unit = parse_rate_limit_duration_unit_from_lit_or_stream(&amount, input)?;
            patch.ttl = Some(CacheDurationSpec { amount, unit });
        } else if input.peek(kw::revalidate) {
            if patch.revalidate.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache revalidate"));
            }
            input.parse::<kw::revalidate>()?;
            patch.revalidate = Some(LitBool::new(true, input.span()));
        } else if input.peek(kw::on_error) {
            if patch.on_error.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache on_error"));
            }
            input.parse::<kw::on_error>()?;
            patch.on_error = Some(if input.peek(kw::ignore) {
                input.parse::<kw::ignore>()?;
                CacheOnErrorSpec::Ignore
            } else if input.peek(kw::serve_stale) {
                input.parse::<kw::serve_stale>()?;
                CacheOnErrorSpec::ServeStale
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `ignore` or `serve_stale` after `on_error`",
                ));
            });
        } else if input.peek(kw::capacity) {
            if patch.capacity.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate cache capacity",
                ));
            }
            input.parse::<kw::capacity>()?;
            let amount: LitInt = input.parse()?;
            let unit = parse_cache_capacity_unit(input, &amount)?;
            patch.capacity = Some(CacheCapacitySpec { amount, unit });
        } else if input.peek(kw::max_body) {
            if patch.max_body.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate cache max_body",
                ));
            }
            input.parse::<kw::max_body>()?;
            let amount: LitInt = input.parse()?;
            let unit = parse_cache_size_unit(input, &amount)?;
            patch.max_body = Some(CacheSizeSpec { amount, unit });
        } else if input.peek(kw::shared) {
            if patch.shared.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate cache shared mode",
                ));
            }
            let token = input.parse::<kw::shared>()?;
            patch.shared = Some(token.span);
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in cache policy block; cache policy blocks support only `http`, `ttl`, `revalidate`, `stale_on_error`, `on_error`, `capacity`, `max_body`, and `shared`",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(patch)
}

fn parse_cache_capacity_unit(
    input: ParseStream<'_>,
    amount: &LitInt,
) -> Result<CacheCapacityUnit> {
    if input.is_empty() {
        return Err(syn::Error::new(
            amount.span(),
            "expected `entries` after cache capacity amount",
        ));
    }
    if input.peek(kw::entries) {
        input.parse::<kw::entries>()?;
        Ok(CacheCapacityUnit::Entries)
    } else {
        let tt: TokenTree = input.parse()?;
        Err(syn::Error::new(
            tt.span(),
            "expected `entries` after cache capacity amount",
        ))
    }
}

fn parse_cache_size_unit(input: ParseStream<'_>, amount: &LitInt) -> Result<CacheSizeUnit> {
    if input.is_empty() {
        return Err(syn::Error::new(
            amount.span(),
            "expected cache size unit after max_body; expected bytes, kb, kib, mb, mib, gb, or gib",
        ));
    }
    if input.peek(kw::bytes) {
        input.parse::<kw::bytes>()?;
        Ok(CacheSizeUnit::Bytes)
    } else if input.peek(kw::kb) {
        input.parse::<kw::kb>()?;
        Ok(CacheSizeUnit::Kb)
    } else if input.peek(kw::kib) {
        input.parse::<kw::kib>()?;
        Ok(CacheSizeUnit::Kib)
    } else if input.peek(kw::mb) {
        input.parse::<kw::mb>()?;
        Ok(CacheSizeUnit::Mb)
    } else if input.peek(kw::mib) {
        input.parse::<kw::mib>()?;
        Ok(CacheSizeUnit::Mib)
    } else if input.peek(kw::gb) {
        input.parse::<kw::gb>()?;
        Ok(CacheSizeUnit::Gb)
    } else if input.peek(kw::gib) {
        input.parse::<kw::gib>()?;
        Ok(CacheSizeUnit::Gib)
    } else {
        let tt: TokenTree = input.parse()?;
        Err(syn::Error::new(
            tt.span(),
            "expected cache size unit after max_body; expected bytes, kb, kib, mb, mib, gb, or gib",
        ))
    }
}

fn parse_rate_limit_duration_unit_from_lit_or_stream(
    amount: &LitInt,
    input: ParseStream<'_>,
) -> Result<RateLimitDurationUnit> {
    match amount.suffix() {
        "s" => Ok(RateLimitDurationUnit::Seconds),
        "m" => Ok(RateLimitDurationUnit::Minutes),
        "" => parse_rate_limit_duration_unit(input),
        _ => Err(syn::Error::new(
            amount.span(),
            "duration shorthand must use `s` or `m`, e.g. `cache 5m`",
        )),
    }
}


