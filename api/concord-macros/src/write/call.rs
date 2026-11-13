//! Delegate the common call surface to the engine's single implementation.
use super::*;
impl Writer<'_> {
    pub(super) fn call_methods(
        &self,
        ty: &TokenStream,
        output: &TokenStream,
        endpoint: &flat::Endpoint<'_>,
        paging: Option<&TokenStream>,
    ) -> TokenStream {
        let root = &self.root;
        let problem = self
            .problem_type()
            .map(|ty| emit!(#ty))
            .unwrap_or_else(|| emit!(()));
        let result_doc = self.result_doc(endpoint);
        let future = ident(if endpoint.pager.is_some() {
            "paged"
        } else {
            "awaitable"
        });
        let conditional = ident(
            if matches!(&endpoint.node.kind, NodeKind::Endpoint(m) if matches!(method(m).as_str(), "GET" | "HEAD"))
            {
                "conditional"
            } else {
                "unconditional"
            },
        );
        let pages = paging.map(|methods| emit!(, pages { #methods }));
        emit_at! { self.client_span =>
            ::concord::__private::call_methods!(#ty, #output, #problem, #root::Result, #result_doc, #future, #conditional #pages);
        }
    }
}
