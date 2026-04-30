#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct DocsIr {
    pub client_docs: Vec<DocBlock>,
    pub scope_docs: Vec<DocBlock>,
    pub endpoint_docs: Vec<DocBlock>,
    pub setter_docs: Vec<DocBlock>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct DocBlock {
    pub target: DocTarget,
    pub summary: String,
    pub sections: Vec<DocSection>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum DocTarget {
    Client,
    Scope(Vec<String>),
    Endpoint(Vec<String>),
    Setter {
        endpoint: Vec<String>,
        field: String,
    },
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct DocSection {
    pub heading: String,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_ir_starts_empty() {
        let docs = DocsIr::default();
        assert!(docs.client_docs.is_empty());
        assert!(docs.endpoint_docs.is_empty());
    }
}
