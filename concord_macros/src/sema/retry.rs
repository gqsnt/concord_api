fn resolve_retry_profiles(
    block: Option<&RetryProfilesBlock>,
) -> Result<BTreeMap<String, RetryConfigResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let mut raw: BTreeMap<String, &RetryProfileDef> = BTreeMap::new();
    for profile in &block.profiles {
        let key = profile.name.to_string();
        if raw.insert(key.clone(), profile).is_some() {
            return Err(syn::Error::new(
                profile.name.span(),
                format!("duplicate retry profile `{key}`"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut stack = Vec::new();
    for profile in &block.profiles {
        resolve_retry_profile(&profile.name, &raw, &mut resolved, &mut stack)?;
    }
    if let Some(default) = &block.default
        && !resolved.contains_key(&default.to_string())
    {
        return Err(syn::Error::new(
            default.span(),
            format!("unknown default retry profile `{default}`"),
        ));
    }

    Ok(resolved)
}

fn resolve_retry_profile(
    name: &Ident,
    raw: &BTreeMap<String, &RetryProfileDef>,
    resolved: &mut BTreeMap<String, RetryConfigResolved>,
    stack: &mut Vec<String>,
) -> Result<RetryConfigResolved> {
    let key = name.to_string();
    if let Some(config) = resolved.get(&key) {
        return Ok(config.clone());
    }
    if stack.iter().any(|item| item == &key) {
        return Err(syn::Error::new(
            name.span(),
            format!("retry profile inheritance cycle involving `{key}`"),
        ));
    }

    let Some(profile) = raw.get(&key) else {
        return Err(syn::Error::new(
            name.span(),
            format!("unknown retry profile `{key}`"),
        ));
    };

    stack.push(key.clone());
    let mut config = if let Some(base) = &profile.extends {
        resolve_retry_profile(base, raw, resolved, stack)?
    } else {
        RetryConfigResolved::default()
    };
    let patch = resolve_retry_patch(&profile.patch)?;
    apply_retry_patch(&mut config, &patch);
    stack.pop();

    resolved.insert(key, config.clone());
    Ok(config)
}

fn resolve_client_retry(
    spec: Option<&RetrySpec>,
    default_profile: Option<&Ident>,
    profiles: &BTreeMap<String, RetryConfigResolved>,
) -> Result<Option<RetryResolved>> {
    if let Some(spec) = spec {
        return resolve_retry_spec(Some(spec), profiles);
    }

    let Some(default_profile) = default_profile else {
        return Ok(None);
    };
    let Some(config) = profiles.get(&default_profile.to_string()) else {
        return Err(syn::Error::new(
            default_profile.span(),
            format!("unknown default retry profile `{default_profile}`"),
        ));
    };
    Ok(Some(RetryResolved::Set(config.clone())))
}

fn resolve_retry_spec(
    spec: Option<&RetrySpec>,
    profiles: &BTreeMap<String, RetryConfigResolved>,
) -> Result<Option<RetryResolved>> {
    match spec {
        None => Ok(None),
        Some(RetrySpec::Off) => Ok(Some(RetryResolved::Clear)),
        Some(RetrySpec::Patch(patch)) => {
            Ok(Some(RetryResolved::Patch(resolve_retry_patch(patch)?)))
        }
        Some(RetrySpec::Profile(name)) => {
            let Some(config) = profiles.get(&name.to_string()) else {
                return Err(syn::Error::new(
                    name.span(),
                    format!("unknown retry profile `{name}`"),
                ));
            };
            Ok(Some(RetryResolved::Set(config.clone())))
        }
    }
}

fn resolve_retry_patch(patch: &RetryPatch) -> Result<RetryPatchResolved> {
    Ok(RetryPatchResolved {
        attempts: patch
            .attempts
            .as_ref()
            .map(resolve_retry_attempts)
            .transpose()?,
        methods: patch
            .methods
            .as_ref()
            .map(|methods| resolve_retry_methods(methods))
            .transpose()?,
        statuses: patch
            .statuses
            .as_ref()
            .map(|statuses| resolve_retry_statuses(statuses))
            .transpose()?,
        transport_errors: patch
            .transport_errors
            .as_ref()
            .map(|kinds| resolve_retry_transport_errors(kinds))
            .transpose()?,
        backoff: patch
            .backoff
            .as_ref()
            .map(resolve_retry_backoff)
            .transpose()?,
        respect_retry_after: patch.respect_retry_after,
        idempotency: patch
            .idempotency
            .as_ref()
            .map(resolve_retry_idempotency)
            .transpose()?,
    })
}

fn apply_retry_patch(config: &mut RetryConfigResolved, patch: &RetryPatchResolved) {
    if let Some(attempts) = patch.attempts {
        config.attempts = attempts;
    }
    if let Some(methods) = &patch.methods {
        config.methods = methods.clone();
    }
    if let Some(statuses) = &patch.statuses {
        config.statuses = statuses.clone();
    }
    if let Some(transport_errors) = &patch.transport_errors {
        config.transport_errors = transport_errors.clone();
    }
    if let Some(backoff) = &patch.backoff {
        config.backoff = backoff.clone();
    }
    if let Some(respect_retry_after) = patch.respect_retry_after {
        config.respect_retry_after = respect_retry_after;
    }
    if let Some(idempotency) = &patch.idempotency {
        config.idempotency = idempotency.clone();
    }
}

fn resolve_retry_attempts(lit: &syn::LitInt) -> Result<u32> {
    let attempts = lit.base10_parse::<u32>()?;
    if attempts == 0 {
        return Err(syn::Error::new(
            lit.span(),
            "retry attempts must be at least 1",
        ));
    }
    Ok(attempts)
}

fn resolve_retry_methods(methods: &[Ident]) -> Result<Vec<Ident>> {
    if methods.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "retry methods list must not be empty",
        ));
    }

    methods
        .iter()
        .map(|method| {
            let name = match method.to_string().as_str() {
                "GET" | "get" => "GET",
                "HEAD" | "head" => "HEAD",
                "POST" | "post" => "POST",
                "PUT" | "put" => "PUT",
                "PATCH" | "patch" => "PATCH",
                "DELETE" | "delete" => "DELETE",
                "OPTIONS" | "options" => "OPTIONS",
                _ => {
                    return Err(syn::Error::new(
                        method.span(),
                        "unknown retry method; expected GET, HEAD, POST, PUT, PATCH, DELETE, or OPTIONS",
                    ));
                }
            };
            Ok(Ident::new(name, method.span()))
        })
        .collect()
}

fn resolve_retry_statuses(statuses: &[syn::LitInt]) -> Result<Vec<u16>> {
    if statuses.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "retry status list must not be empty",
        ));
    }

    statuses
        .iter()
        .map(|status| {
            let value = status.base10_parse::<u16>()?;
            if !(100..=599).contains(&value) {
                return Err(syn::Error::new(
                    status.span(),
                    "retry status must be an HTTP status code in 100..=599",
                ));
            }
            Ok(value)
        })
        .collect()
}

fn resolve_retry_transport_errors(kinds: &[Ident]) -> Result<Vec<Ident>> {
    if kinds.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "retry transport list must not be empty",
        ));
    }

    kinds
        .iter()
        .map(|kind| {
            let variant = match kind.to_string().as_str() {
                "Timeout" | "timeout" => "Timeout",
                "Connect" | "connect" => "Connect",
                "Tls" | "TLS" | "tls" => "Tls",
                "Dns" | "DNS" | "dns" => "Dns",
                "Io" | "IO" | "io" => "Io",
                "Request" | "request" => "Request",
                "Other" | "other" => "Other",
                _ => {
                    return Err(syn::Error::new(
                        kind.span(),
                        "unknown transport retry kind; expected Timeout, Connect, Tls, Dns, Io, Request, or Other",
                    ));
                }
            };
            Ok(Ident::new(variant, kind.span()))
        })
        .collect()
}

fn resolve_retry_backoff(spec: &RetryBackoffSpec) -> Result<RetryBackoffResolved> {
    match spec {
        RetryBackoffSpec::None => Ok(RetryBackoffResolved::None),
    }
}

fn resolve_retry_idempotency(spec: &RetryIdempotencySpec) -> Result<RetryIdempotencyResolved> {
    match spec {
        RetryIdempotencySpec::Header(header) => {
            if header.value().trim().is_empty() {
                return Err(syn::Error::new(
                    header.span(),
                    "retry idempotency header must not be empty",
                ));
            }
            Ok(RetryIdempotencyResolved::Header(header.clone()))
        }
    }
}

