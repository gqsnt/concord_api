// Scopes and endpoints accept documentation, `#[cfg(..)]` and `#[deprecated]` only.
concord::api! {
    pub client Api {
        base "http://x.test";
        #[inline]
        GET item();
    }
}
fn main() {}
