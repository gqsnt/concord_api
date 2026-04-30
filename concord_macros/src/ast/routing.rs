#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Prefix,
    Path,
}

#[derive(Debug, Clone)]
pub struct RouteExpr {
    pub atoms: Vec<RouteAtom>,
}

#[derive(Debug, Clone)]
pub enum RouteAtom {
    Static(LitStr),
    Ref(ScopedRef),
    Fmt(FmtSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefScope {
    Cx,
    Ep,
    Auth,
}
#[derive(Debug, Clone)]
pub struct ScopedRef {
    pub scope: RefScope,
    pub ident: Ident,
    pub explicit: bool,
}

