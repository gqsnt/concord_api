enum CacheDecl {
    Profiles(CacheProfilesBlock),
    Spec(CacheSpec),
}

fn parse_cache_decl(input: ParseStream<'_>) -> Result<CacheDecl> {
    let fork = input.fork();
    fork.parse::<kw::cache>()?;
    if fork.peek(token::Brace) {
        let content;
        braced!(content in fork);
        if content.peek(kw::profile) || content.peek(kw::default) {
            return Ok(CacheDecl::Profiles(parse_cache_profiles_decl(input)?));
        }
    }
    Ok(CacheDecl::Spec(parse_cache_spec(input)?))
}

fn parse_cache_profiles_decl(input: ParseStream<'_>) -> Result<CacheProfilesBlock> {
    input.parse::<kw::cache>()?;
    let content;
    braced!(content in input);

    let mut profiles = Vec::new();
    let mut default = None;
    while !content.is_empty() {
        if content.peek(kw::profile) {
            content.parse::<kw::profile>()?;
            let name: Ident = content.parse()?;
            let extends = if content.peek(kw::extends) {
                content.parse::<kw::extends>()?;
                Some(content.parse()?)
            } else {
                None
            };
            let body;
            braced!(body in content);
            profiles.push(CacheProfileDef {
                name,
                extends,
                patch: parse_cache_patch_body(&body)?,
            });
        } else if content.peek(kw::default) {
            if default.is_some() {
                return Err(syn::Error::new(content.span(), "duplicate cache default"));
            }
            content.parse::<kw::default>()?;
            default = Some(content.parse()?);
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in cache block",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(CacheProfilesBlock { profiles, default })
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
        } else if input.peek(kw::capacity) {
            if patch.capacity.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache capacity"));
            }
            input.parse::<kw::capacity>()?;
            patch.capacity = Some(parse_cache_capacity(input)?);
        } else if input.peek(kw::max_body) {
            if patch.max_body.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache max_body"));
            }
            input.parse::<kw::max_body>()?;
            patch.max_body = Some(parse_cache_size(input)?);
        } else if input.peek(kw::revalidate) {
            if patch.revalidate.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache revalidate"));
            }
            input.parse::<kw::revalidate>()?;
            if !input.peek(LitBool) {
                return Err(syn::Error::new(
                    input.span(),
                    "expected `true` or `false` after `revalidate`",
                ));
            }
            patch.revalidate = Some(input.parse::<LitBool>()?);
        } else if input.peek(kw::shared) {
            if patch.shared.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache shared"));
            }
            input.parse::<kw::shared>()?;
            patch.shared = Some(input.parse::<LitBool>()?);
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
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in cache policy block",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(patch)
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

fn parse_cache_capacity(input: ParseStream<'_>) -> Result<CacheCapacitySpec> {
    let amount: LitInt = input.parse()?;
    if input.peek(kw::entries) {
        input.parse::<kw::entries>()?;
        Ok(CacheCapacitySpec::Entries { amount })
    } else {
        let unit = parse_cache_size_unit(input)?;
        Ok(CacheCapacitySpec::Bytes(CacheSizeSpec { amount, unit }))
    }
}

fn parse_cache_size(input: ParseStream<'_>) -> Result<CacheSizeSpec> {
    let amount: LitInt = input.parse()?;
    let unit = parse_cache_size_unit(input)?;
    Ok(CacheSizeSpec { amount, unit })
}

fn parse_cache_size_unit(input: ParseStream<'_>) -> Result<CacheSizeUnit> {
    if input.peek(kw::bytes) {
        input.parse::<kw::bytes>()?;
        Ok(CacheSizeUnit::Bytes)
    } else if input.peek(kw::kb) {
        input.parse::<kw::kb>()?;
        Ok(CacheSizeUnit::KiB)
    } else if input.peek(kw::kib) {
        input.parse::<kw::kib>()?;
        Ok(CacheSizeUnit::KiB)
    } else if input.peek(kw::mb) {
        input.parse::<kw::mb>()?;
        Ok(CacheSizeUnit::MiB)
    } else if input.peek(kw::mib) {
        input.parse::<kw::mib>()?;
        Ok(CacheSizeUnit::MiB)
    } else if input.peek(kw::gb) {
        input.parse::<kw::gb>()?;
        Ok(CacheSizeUnit::GiB)
    } else if input.peek(kw::gib) {
        input.parse::<kw::gib>()?;
        Ok(CacheSizeUnit::GiB)
    } else {
        let tt: TokenTree = input.parse()?;
        Err(syn::Error::new(
            tt.span(),
            "expected cache size unit `bytes`, `kb`/`kib`, `mb`/`mib`, or `gb`/`gib`",
        ))
    }
}

