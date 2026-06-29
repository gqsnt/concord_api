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
    Remove {
        key: KeySpec,
    },
    Set {
        key: KeySpec,
        value: PolicyValue,
        op: SetOp,
    },
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

/// `Json<T>` (family marker = `Json`, decoded/body type = `T`)
#[derive(Debug, Clone)]
pub struct RawIoSpec {
    pub marker: Type,
    pub enc: Path,
    pub ty: Type,
    pub args: Vec<Type>,
    pub had_angle_args: bool,
}

pub type RawRequestIo = Option<RawIoSpec>;
pub type RawResponseIo = RawIoSpec;

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
    pub require_all: bool,     // all referenced optional pieces must be present
    pub pieces: Vec<FmtPiece>, // ["...", vars.x, ...]
}

#[derive(Debug, Clone)]
pub enum FmtPiece {
    Lit(LitStr),
    Ref(ScopedRef),
}
