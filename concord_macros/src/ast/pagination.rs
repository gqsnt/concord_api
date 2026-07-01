#[derive(Debug)]
pub struct PaginateSpec {
    pub endpoint_state: bool,
    pub ctrl_ty: Path,
    pub bindings_ty: Option<Type>,
    pub assigns: Vec<PaginateAssign>,
}

#[derive(Debug)]
pub struct PaginateAssign {
    pub key: Ident,
    pub value: Expr,
}

