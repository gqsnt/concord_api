#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct FacadeIr {
    pub client_name: String,
    pub scopes: Vec<FacadeScope>,
    pub endpoints: Vec<FacadeEndpoint>,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeScope {
    pub public_name: String,
    pub rust_type_name: String,
    pub parent_path: Vec<String>,
    pub methods: Vec<FacadeMethod>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeEndpoint {
    pub target_endpoint: String,
    pub public_method: String,
    pub scope_path: Vec<String>,
    pub required_args: Vec<FacadeArg>,
    pub setters: Vec<FacadeSetter>,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeMethod {
    pub public_name: String,
    pub target_scope_path: Vec<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeArg {
    pub name: String,
    pub ty: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeSetter {
    pub field: String,
    pub ty: String,
    pub forms: Vec<SetterForm>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum SetterForm {
    Set,
    SetOptional,
    Clear,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeDoc {
    pub summary: String,
    pub details: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setter_forms_match_current_public_surface() {
        let setter = FacadeSetter {
            field: "limit".to_string(),
            ty: "u64".to_string(),
            forms: vec![SetterForm::Set, SetterForm::SetOptional, SetterForm::Clear],
        };

        assert_eq!(setter.forms.len(), 3);
        assert!(setter.forms.contains(&SetterForm::Clear));
    }
}
