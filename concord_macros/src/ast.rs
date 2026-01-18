use proc_macro2::Span;
use syn::{Expr, Ident, LitStr, Path, Type};
use syn::spanned::Spanned;

#[derive(Debug)]
pub struct ApiFile {
    pub client: ClientDef,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct ClientDef {
    pub name: Ident,
    pub scheme: SchemeLit,
    pub host: LitStr,
    pub policy: PolicyBlocks,
    pub vars: Option<VarsBlock>,
    pub auth_vars: Option<VarsBlock>,
}

#[derive(Debug)]
pub struct VarsBlock {
    pub decls: Vec<VarDeclNoWire>,
}

#[derive(Debug, Clone, Copy)]
pub enum SchemeLit {
    Http,
    Https,
}

#[derive(Debug)]
pub enum Item {
    Layer(LayerDef),
    Endpoint(EndpointDef),
}

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
    Var(TemplateVarDecl),
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
}

#[derive(Debug)]
pub struct LayerDef {
    pub kind: LayerKind,
    pub route: RouteExpr,
    pub policy: PolicyBlocks,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct EndpointDef {
    pub method: Ident, // "GET", "POST", ...
    pub name: Ident,
    pub route: RouteExpr,

    pub policy: PolicyBlocks,

    pub paginate: Option<PaginateSpec>,
    pub body: Option<CodecSpec>,

    pub response: CodecSpec,
    pub map: Option<MapSpec>,

}

#[derive(Debug)]
pub struct PaginateSpec {
    pub ctrl_ty: Path,
    pub assigns: Vec<PaginateAssign>,
}

#[derive(Debug)]
pub struct PaginateAssign {
    pub key: Ident,
    pub value: Expr,
}


#[derive(Debug, Clone)]
pub struct MapSpec {
    pub out_ty: Type,
    pub body: Expr, // expression utilisant `r`
}

#[derive(Debug, Default)]
pub struct PolicyBlocks {
    pub headers: Option<PolicyBlock>,
    pub query: Option<PolicyBlock>,
    pub timeout: Option<Expr>,
}

#[derive(Debug)]
pub struct PolicyBlock {
    pub stmts: Vec<PolicyStmt>,
}

#[derive(Debug)]
pub enum PolicyStmt {
    Remove { key: KeySpec },
    Set { key: KeySpec, value: PolicyValue, op: SetOp },
    Bind { key: KeySpec, decl: VarDeclNoWire },
    BindShort { ident_key: Ident, decl: VarDeclShort },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOp {
    Set,
    Push, // query only
}

#[derive(Debug)]
pub enum KeySpec {
    Ident(Ident),
    Str(LitStr),
}

/// `as x_debug?: bool = true`
#[derive(Debug, Clone)]
pub struct VarDeclNoWire {
    pub rust: Ident,
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// `page_cursor?: String = "x".into()`
#[derive(Debug, Clone)]
pub struct VarDeclShort {
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// `{wire as rust?: Ty = default}`
#[derive(Debug, Clone)]
pub struct TemplateVarDecl {
    pub wire: Ident,
    pub rust: Ident,
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

/// `Json<T>` (encoding type = `Json`, decoded/body type = `T`)
#[derive(Debug, Clone)]
pub struct CodecSpec {
    pub enc: Path,
    pub ty: Type,
}





#[derive(Debug)]
pub enum PolicyValue {
    Expr(Expr),
    Fmt(FmtSpec),
}

impl PolicyValue {
    #[inline]
    pub fn span(&self) -> Span {
        match self {
            PolicyValue::Expr(e) => e.span(),
            PolicyValue::Fmt(f) => f.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FmtSpec {
    pub span: Span,
    pub require_all: bool,     // fmt? => true
    pub pieces: Vec<FmtPiece>, // ["...", {x:u32}, ...]
}

#[derive(Debug, Clone)]
pub enum FmtPiece {
    Lit(LitStr),
    Var(TemplateVarDecl), // réutilise déjà votre parser de `{wire as rust?: Ty = default}`
    Ref(ScopedRef),
}