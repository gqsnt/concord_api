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
    pub(crate) endpoint: Endpoint,
}

pub(crate) struct Endpoint {
    pub(crate) name: Ident,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) path: Vec<PathAtom>,
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
