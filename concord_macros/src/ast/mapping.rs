#[derive(Debug, Clone)]
pub struct MapSpec {
    pub out_ty: Type,
    pub body: Expr, // expression utilisant `r`
}

