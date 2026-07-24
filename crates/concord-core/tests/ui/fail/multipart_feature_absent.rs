use concord_core::contracts::{MultipartBody, MultipartFactory, MultipartPart};

fn main() {
    let _ = std::mem::size_of::<MultipartBody>();
    let _ = std::mem::size_of::<MultipartPart>();
    fn accepts_factory<T: MultipartFactory>() {}
    let _ = accepts_factory::<()>;
}
