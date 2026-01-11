use proc_macro::TokenStream;

mod ast;
mod codegen;
mod emit_helpers;
mod kw;
mod parse;
mod sema;

#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();
    let ast = match syn::parse2::<ast::ApiFile>(input2) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let ir = match sema::analyze(ast) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    codegen::emit(ir).into()
}
