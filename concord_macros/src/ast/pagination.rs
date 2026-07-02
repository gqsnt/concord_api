#[derive(Debug)]
pub struct PaginateSpec {
    pub ctrl_ty: Type,
    pub assigns: Vec<PaginateAssign>,
}

#[derive(Debug)]
pub struct PaginateAssign {
    pub key: Ident,
    pub value: Expr,
}

