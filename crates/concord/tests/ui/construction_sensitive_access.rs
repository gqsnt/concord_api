// These clients are declared in a separate crate: sensitivity withholds every projection.
use concord_construction_fixture::{Native, Sensitive};
fn base(api: &Sensitive) { let _ = api.base_url(); }
fn value(api: &Sensitive) { let _ = api.origin(); }
fn native(api: &Native) { let _ = api.owned().as_bytes(); }
fn conditional(api: &Native) { let _ = api.conditional(); }
fn main() {}
