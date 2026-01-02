use proc_macro::TokenStream;

mod codegen;
mod ir;
mod parse;

#[proc_macro]
pub fn api_client(input: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(input as parse::ApiFile);
    match ir::lower(ast).and_then(codegen::emit) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
