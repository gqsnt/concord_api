// V3-L09 / VALUE-04 (negative half): there is no numeric widening, no domain
// parsing from `&str`, no array/slice-to-Vec or element conversion, no
// recursive optional conversion, no `Display` fallback, and exact/conditional
// controls take exact `Option<T>` — never `Option<impl IntoValue<T>>`. The
// positive half is pass/language/value_conversions.rs; the sealed
// registration half is value_conversion_registration_sealed.rs.
use concord::{AtomError, Binary, BoundedAtomWriter, Bytes, RequestAtom, api};

pub struct ItemId(pub String);

impl RequestAtom for ItemId {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(&self.0)
    }
}

api! {
    client ConversionApi {
        base "https://conversions.example.test";
        values {
            query region?: String;
            query limit?: u64;
        }

        GET item(id: crate::ItemId, query("tag") each tags: Vec<String>) -> Bytes via Binary {
            path "items/{id}";
        }
    }
}

fn main() {
    let narrow: u32 = 50;
    let _ = ConversionApi::builder().set_limit(narrow);
    // VALUE-04: no `Display` fallback. `u32: Display` does not make it a
    // `String` payload; the caller writes `7.to_string()` explicitly.
    let _ = ConversionApi::builder().set_region(7_u32);
    let _ = ConversionApi::builder().replace_region(Some("eu"));
    let _ = ConversionApi::builder().set_region_if_some(Some("eu"));

    let client = ConversionApi::new().unwrap();
    let _ = client.item("42", Vec::new());
    let _ = client.item(ItemId("42".to_owned()), ["a".to_owned()]);
    let _ = client.item(ItemId("42".to_owned()), vec!["a"]);
}
