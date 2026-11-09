mod sdk {
    concord::api! {
        pub client Store {
            base "http://127.0.0.1";
            GET list();
        }
    }
}
pub use sdk::__StoreOperation;
fn main() {}
