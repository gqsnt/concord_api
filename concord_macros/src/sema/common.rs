use super::*;

pub(super) fn unknown_name_message<T>(
    kind: &str,
    name: &Ident,
    available: &BTreeMap<String, T>,
) -> String {
    unknown_name_message_from_keys(kind, &name.to_string(), available.keys().cloned())
}

pub(super) fn unknown_scoped_name_message<T>(
    kind: &str,
    prefix: &str,
    name: &Ident,
    available: &BTreeMap<String, T>,
) -> String {
    let scoped = format!("{prefix}.{name}");
    unknown_name_message_from_keys(
        kind,
        &scoped,
        available.keys().map(|key| format!("{prefix}.{key}")),
    )
}

pub(super) fn unknown_name_message_from_keys(
    kind: &str,
    name: &str,
    available: impl Iterator<Item = String>,
) -> String {
    let available = available.collect::<Vec<_>>();
    let mut message = format!("unknown {kind} `{name}`");
    if let Some(suggestion) = best_name_suggestion(name, available.iter()) {
        message.push_str(&format!("\ndid you mean `{suggestion}`?"));
    }
    if available.is_empty() {
        message.push_str(&format!("\nno {kind}s are declared"));
    } else {
        let names = available
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ");
        message.push_str(&format!("\navailable {kind}s: {names}"));
    }
    message
}

pub(super) fn best_name_suggestion<'a>(
    needle: &str,
    candidates: impl Iterator<Item = &'a String>,
) -> Option<String> {
    candidates
        .map(|candidate| {
            (
                levenshtein(needle, candidate),
                candidate.as_str().len().abs_diff(needle.len()),
                candidate.clone(),
            )
        })
        .filter(|(distance, len_delta, _)| *distance <= 3 || (*distance <= 4 && *len_delta <= 2))
        .min_by_key(|(distance, len_delta, _)| (*distance, *len_delta))
        .map(|(_, _, candidate)| candidate)
}

pub(super) fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars = b.chars().collect::<Vec<_>>();
    let mut costs = (0..=b_chars.len()).collect::<Vec<_>>();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = costs[0];
        costs[0] = i + 1;
        for (j, &cb) in b_chars.iter().enumerate() {
            let insert = costs[j + 1] + 1;
            let delete = costs[j] + 1;
            let replace = previous + usize::from(ca != cb);
            previous = costs[j + 1];
            costs[j + 1] = insert.min(delete).min(replace);
        }
    }
    costs[b_chars.len()]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PolicyOwner {
    Client,
    Endpoint,
    Layer,
}

pub(super) fn upsert_var(
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
        if let (Some(prev_default), Some(default)) = (prev.default.as_ref(), default)
            && prev_default != default
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
