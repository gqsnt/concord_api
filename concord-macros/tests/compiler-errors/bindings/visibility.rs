mod outer {
    pub mod restricted {
        concord::api! {
            pub(in super) client Relative {
                base "https://relative.test";
                scope middle { scope inner { GET get(); } }
            }
        }
    }
    pub fn inside() { let _ = restricted::Relative::new().middle().inner().get(); let _: Option<restricted::relative::middle::Inner> = None; }
}
fn main() {
    let _ = outer::restricted::Relative::new();
    let _: Option<outer::restricted::relative::middle::Inner> = None;
}
