//! The derive's bound and formatter resolve through an explicitly named Concord dependency.

// Reuse the bridge so this isolated consumer check needs no added dependency.
use concord4::__macro::{Serialize, Serializer};

#[derive(concord4::WireDisplay)]
#[wire_display(crate = concord4)]
struct Sort;

impl Serialize for Sort {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit_variant("Sort", 0, "long-running")
    }
}

// A module-qualified path also works, including with a generic type and an existing bound.
mod facade {
    pub use concord4::*;
}

#[derive(concord4::WireDisplay)]
#[wire_display(crate = crate::facade)]
struct Id<T: Copy>(T);

impl<T: Copy + Serialize> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

concord4::api! {
    client Api {
        base "http://renamed.invalid";
        GET list(query sort: Sort, id: Id<u64>) { path "items/{id}"; }
    }
}

#[test]
fn wire_display_through_the_renamed_crate() {
    assert_eq!(Sort.to_string(), "long-running");
    assert_eq!(Id(7_u64).to_string(), "7");
    let request = Api::new(concord4::reqwest::Client::new())
        .list(Sort, Id(7))
        .prepare()
        .unwrap();
    assert_eq!(
        request.request().url().as_str(),
        "http://renamed.invalid/items/7?sort=long-running"
    );
}
