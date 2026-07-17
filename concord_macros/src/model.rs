use syn::{Ident, LitStr, Type};

pub(crate) struct Api {
    pub(crate) client: Client,
}

pub(crate) struct Client {
    pub(crate) name: Ident,
    pub(crate) base: LitStr,
    pub(crate) scope: Scope,
}

pub(crate) struct Scope {
    pub(crate) name: Ident,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) path: Vec<PathAtom>,
    pub(crate) endpoints: Vec<Endpoint>,
}

pub(crate) struct Endpoint {
    pub(crate) method: MethodKind,
    pub(crate) name: Ident,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) body: Option<CodecKind>,
    pub(crate) path: Vec<PathAtom>,
    pub(crate) response: ResponseKind,
}

#[derive(Clone, Copy)]
pub(crate) enum MethodKind {
    Get,
    Post,
}

pub(crate) enum ResponseKind {
    Bytes,
    NoContent,
    Buffered(CodecKind),
}

pub(crate) enum CodecKind {
    Text,
    Json(Type),
    Custom(Type),
}

pub(crate) struct Parameter {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
}

pub(crate) enum PathAtom {
    Static(LitStr),
    Scope(Ident),
    Endpoint(Ident),
}

pub(crate) fn to_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let characters = name.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(name.len());
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_uppercase() {
            let previous_is_lowercase = index > 0 && characters[index - 1].is_lowercase();
            let next_is_lowercase = characters
                .get(index + 1)
                .is_some_and(|next| next.is_lowercase());
            if index > 0 && (previous_is_lowercase || next_is_lowercase) {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}
