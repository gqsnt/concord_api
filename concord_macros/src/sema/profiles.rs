use std::collections::BTreeSet;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Profile<T> {
    pub name: String,
    pub extends: Option<String>,
    pub value: T,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ProfileSet<T> {
    pub profiles: std::collections::BTreeMap<String, Profile<T>>,
    pub defaults: Vec<String>,
}

impl<T> Default for ProfileSet<T> {
    fn default() -> Self {
        Self {
            profiles: std::collections::BTreeMap::new(),
            defaults: Vec::new(),
        }
    }
}

#[allow(dead_code)]
pub trait ProfileValue: Clone {
    fn empty() -> Self;
    fn merge(parent: Self, child: Self) -> Self;
    fn validate(&self) -> syn::Result<()>;
}

#[allow(dead_code)]
pub fn resolve_profile_set<T, I>(
    label: &'static str,
    raw_profiles: I,
    defaults: Vec<syn::Ident>,
) -> syn::Result<std::collections::BTreeMap<String, T>>
where
    T: ProfileValue,
    I: IntoIterator<Item = (syn::Ident, Option<syn::Ident>, T)>,
{
    let mut set = ProfileSet::<T>::default();
    for (name, extends, value) in raw_profiles {
        let key = name.to_string();
        if set.profiles.contains_key(&key) {
            return Err(syn::Error::new(
                name.span(),
                format!("duplicate {label} profile `{key}`"),
            ));
        }
        set.profiles.insert(
            key.clone(),
            Profile {
                name: key,
                extends: extends.map(|ident| ident.to_string()),
                value,
            },
        );
    }
    set.defaults = defaults.into_iter().map(|ident| ident.to_string()).collect();

    for default in &set.defaults {
        if !set.profiles.contains_key(default) {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("unknown default {label} profile `{default}`"),
            ));
        }
    }

    let mut resolved = std::collections::BTreeMap::new();
    let keys = set.profiles.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        let value = resolve_one_profile(label, &set, &key, &mut BTreeSet::new(), &mut resolved)?;
        value.validate()?;
    }
    Ok(resolved)
}

#[allow(dead_code)]
fn resolve_one_profile<T: ProfileValue>(
    label: &'static str,
    set: &ProfileSet<T>,
    key: &str,
    visiting: &mut BTreeSet<String>,
    resolved: &mut std::collections::BTreeMap<String, T>,
) -> syn::Result<T> {
    if let Some(value) = resolved.get(key) {
        return Ok(value.clone());
    }
    let profile = set.profiles.get(key).ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("unknown {label} profile `{key}`"),
        )
    })?;
    if !visiting.insert(key.to_string()) {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("{label} profile inheritance cycle involving `{key}`"),
        ));
    }
    let parent = match &profile.extends {
        Some(parent) => resolve_one_profile(label, set, parent, visiting, resolved)?,
        None => T::empty(),
    };
    let value = T::merge(parent, profile.value.clone());
    visiting.remove(key);
    resolved.insert(key.to_string(), value.clone());
    Ok(value)
}
