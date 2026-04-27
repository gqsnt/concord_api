fn policy_uses_cache(policy: &PolicyBlocksResolved) -> bool {
    policy
        .cache
        .as_ref()
        .is_some_and(|cache| matches!(cache, CacheResolved::Set(_) | CacheResolved::Patch(_)))
}

fn endpoint_uses_cache(endpoint: &ResolvedEndpoint) -> bool {
    endpoint.policy.scopes.iter().any(policy_uses_cache)
        || policy_uses_cache(&endpoint.policy.endpoint)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyOwner {
    Client,
    Endpoint,
    Layer,
}

fn upsert_var(
    out: &mut BTreeMap<String, VarInfo>,
    rust: &Ident,
    optional: bool,
    ty: &Type,
    default: Option<&Expr>,
) -> Result<()> {
    let k = rust.to_string();
    if let Some(prev) = out.get(&k) {
        // strict duplicate declaration consistency
        if prev.optional != optional {
            return Err(syn::Error::new(
                rust.span(),
                format!("var `{}` redefined with different optionality", k),
            ));
        }
        if prev.ty != *ty {
            return Err(syn::Error::new(
                rust.span(),
                format!("var `{}` redefined with different type", k),
            ));
        }
        // default consistency: allow same tokens or missing
        if prev.default.is_some()
            && default.is_some()
            && prev.default.as_ref().unwrap() != default.unwrap()
        {
            return Err(syn::Error::new(
                rust.span(),
                format!("var `{}` redefined with different default", k),
            ));
        }
        return Ok(());
    }

    out.insert(
        k,
        VarInfo {
            rust: rust.clone(),
            optional,
            ty: ty.clone(),
            default: default.cloned(),
        },
    );
    Ok(())
}

