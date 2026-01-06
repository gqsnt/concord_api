use proc_macro::TokenStream;

mod codegen;
mod ir;
mod parse;

#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(input as parse::ApiFile);
    match ir::lower(ast).and_then(codegen::emit) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(ControllerBuild)]
pub fn derive_controller_build(input: TokenStream) -> TokenStream {
    derive_controller_build::derive(input)
}

mod derive_controller_build;
