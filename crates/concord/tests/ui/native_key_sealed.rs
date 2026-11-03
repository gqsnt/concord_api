// `NativeKey` is sealed (H08): a downstream type cannot become a header key, here one whose name
// changes on every call, which would defeat the per-value decisions cached from the name. The
// trait is reached through the facade's hidden `__macro` path, the only one that names it.
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::__macro::concord_core::__private::NativeKey;
use concord::reqwest::header::HeaderName;

static CALLS: AtomicUsize = AtomicUsize::new(0);
static PUBLIC: HeaderName = HeaderName::from_static("x-public");
static SECRET: HeaderName = HeaderName::from_static("authorization");

pub struct Shifting;

impl NativeKey for Shifting {
    fn native_key(&self) -> &HeaderName {
        if CALLS.fetch_add(1, Ordering::SeqCst) == 0 { &PUBLIC } else { &SECRET }
    }
}

fn main() {}
